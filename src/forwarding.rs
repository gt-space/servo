use std::{
	collections::HashSet,
	future::Future,
	io,
	net::SocketAddr,
	sync::Arc,
	time::{self, SystemTime},
};

use tokio::{
	net::UdpSocket,
	sync::{Mutex, broadcast::{self, Sender}},
	time::{Duration, MissedTickBehavior, Instant}
};

use crate::Database;
use log::{error, warn};
use rusqlite::functions;

/// A struct which can forward messages incoming on a single UDP port to multiple external sockets 
pub struct ForwardingAgent {
	targets: Mutex<HashSet<SocketAddr>>,
	log_after_duration: Duration,
	log_after_size: usize,
	frame_sender: Sender<(u64, Vec<u8>)>,
}

impl ForwardingAgent {
	/// Constructs a new ForwardingAgent with a 1MB log-after size and 500ms log-after duration.
	pub fn new() -> Self {
		ForwardingAgent {
			targets: Mutex::new(HashSet::new()),
			log_after_duration: Duration::from_millis(500),
			log_after_size: 1_000_000, // 1MB
			frame_sender: broadcast::channel(20).0,
		}
	}

	/// Returns the current duration after which a log must be committed to the database (default 500ms).
	pub fn log_after_duration(&self) -> Duration {
		return self.log_after_duration.clone();
	}

	/// Sets the duration after which a log must be committed to the database (default 500ms).
	/// This method must be called before `ForwardingAgent::log_frames` to have the intended effect.
	///
	/// Method will panic if log duration is set to less than 5ms, as it would cause logs to be
	/// unnecessarily small, causing compression results to be poor and possibly making the
	/// database fall behind.
	pub fn set_log_after_duration(&mut self, duration: Duration) {
		if duration.as_millis() < 5 {
			error!("log_after_duration cannot be set < 5ms");
			return;
		}

		self.log_after_duration = duration;
	}

	/// Returns the current size (in bytes) after which a log must be committed to the database (default 1MB).
	pub fn log_after_size(&self) -> usize {
		return self.log_after_size;
	}


	/// Sets the size (in bytes) after which a log must be committed to the database (default 1MB).
	/// This method must be called before `ForwardingAgent::log_frames` to have the intended effect.
	///
	/// Method will panic if log size is set to less than 4KB, as compression under of blocks under
	/// 4KB yields poor results. It would also cause performance issues on targets with slower
	/// file IO, possibly causing the database to permanently fall behind and crash.
	pub fn set_log_after_size(&mut self, size: usize) {
		if size < 4_000 {
			error!("log_after_size cannot be set < 4KB");
			return;
		}

		self.log_after_size = size;
	}

	/// Constructs a closure which can be passed to `rusqlite::Connection::create_scalar_function` to asynchronously
	/// update the `ForwardingAgent`'s targets based on what is stored in a database.
	/// In SQLite queries, this function can be named anything but must take in two parameters:
	/// 1) A string which is the socket address of the target (ex. '127.0.0.1:32000')
	/// 2) 0 for removal of the target, or 1 for inclusion of the target
	/// 
	/// This method must be called on a `Arc<ForwardingAgent>` because it constructs a `Weak<ForwardingAgent>` to be
	/// used in the closure. This satisfies the borrow checker while ensuring no memory leaks from undying references.
	/// 
	/// # Example
	/// 
	/// ```
	/// let database = rusqlite::Connection::open_in_memory()?;
	/// database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, forwarding_agent.update_targets())?;
	/// ```
	/// 
	pub fn update_targets(self: &Arc<Self>) -> impl Fn(&functions::Context) -> rusqlite::Result<bool> {
		// Notice that std::sync::Mutex is being used here rather than tokio::sync::Mutex. This is necessitated
		// due to the closure's requirement that captured variables implement UnwindSafe, which for Mutexes, requires
		// it to be poison-able. Tokio's Mutex is not poison-able, since this issue doesn't occur in async code. Yes,
		// this means that the inner 'targets' is wrapped in a tokio Mutex and the outer ForwardingAgent is wrapped
		// in a std Mutex, which is strange. But it's necessary to have these objects accessible in both multi-threaded
		// (rusqlite's choice, not mine) and async code.
		//
		// tl;dr remove the Mutex and watch the compiler have a heart attack at the call site
		let weak_self = std::sync::Mutex::new(Arc::downgrade(self));

		move |context| {
			let weak_self = weak_self
				.lock()
				.expect("forwarding agent mutex has been poisoned");

			if let Some(strong_self) = weak_self.upgrade() {
				let target_address = context
					.get::<String>(0)?
					.parse()
					.unwrap();

				let should_add = context.get::<bool>(1)?;

				tokio::spawn(async move {
					if should_add {
						strong_self.targets
							.lock()
							.await
							.insert(target_address);
					} else {
						strong_self.targets
							.lock()
							.await
							.remove(&target_address);
					}
				});

				Ok(true)
			} else {
				Err(rusqlite::Error::UserFunctionError("forwarding agent has been dropped".into()))
			}
		}
	}


	/// Accumulates and logs frames being forwarded into the 'DataLogs' table of the given database.
	/// 
	/// Although it takes a strong reference to the database, it immediately downgrades it to a weak reference and stops
	/// execution if/when the database is dropped. 
	/// 
	/// # Example
	/// 
	/// ```
	/// let database = Arc::new(Mutex::new(SqlConnection::open(":memory:")));
	/// let forwarding_agent = ForwardingAgent::new();
	/// 
	/// tokio::spawn(forwarding_agent.forward());
	/// tokio::spawn(forwarding_agent.log_frames(&database));
	/// ```
	pub fn log_frames(&self, database: &Database) -> impl Future<Output = ()> {
		let weak_database = Arc::downgrade(database);

		let mut rx = self.frame_sender.subscribe();
		let log_after_size = self.log_after_size;
		let log_after_duration = self.log_after_duration;

		async move {
			let mut accumulation_buffer = Vec::with_capacity(log_after_size);
			let mut frame_split_indices = Vec::new();

			let mut last_write_time = Instant::now();

			loop {
				match rx.try_recv() {
					Ok((time_received, mut frame_buffer)) => {
						accumulation_buffer.extend(time_received.to_le_bytes());
						accumulation_buffer.append(&mut frame_buffer);

						frame_split_indices.extend((accumulation_buffer.len() as u64).to_le_bytes());
					},
					Err(broadcast::error::TryRecvError::Closed) => break,
					Err(broadcast::error::TryRecvError::Lagged(count)) => {
						warn!("ForwardingAgent::log_frames lagged {count} frames");
						continue;
					},
					_ => continue,
				}

				let since_last_write = Instant::now()
					.duration_since(last_write_time);

				if accumulation_buffer.len() > log_after_size || since_last_write > log_after_duration {
					if let Some(database) = weak_database.upgrade() {
						database.lock().await
							.execute(
								"INSERT INTO DataLogs (raw_accumulated, frame_split_indices) VALUES (?1, ?2)",
								rusqlite::params![accumulation_buffer, frame_split_indices]
							)
							.unwrap();
						
						accumulation_buffer.clear();
						frame_split_indices.clear();

						last_write_time = Instant::now();
					} else {
						break;
					}
				}
			}
		}
	}

	/// Continuously forwards incoming UDP datagrams on port 7201 to all available targets. Targets are updated indirectly
	/// by a database using the `ForwardingAgent::update_targets` method.
	/// 
	/// # Example
	/// 
	/// ```
	/// let forwarding_agent = ForwardingAgent::new();
	/// tokio::spawn(forwarding_agent.forward());
	/// ```
	/// 
	pub fn forward(self: &Arc<Self>) -> impl Future<Output = io::Result<()>> {
		let weak_self = Arc::downgrade(self);

		async move {
			let socket = UdpSocket::bind("0.0.0.0:7201").await?;

			let mut frame_buffer = vec![0; 521];

			while let Some(strong_self) = weak_self.upgrade() {
				let targets = strong_self.targets.lock().await;

				if targets.len() == 0 {
					continue;
				}

				match socket.recv_from(&mut frame_buffer).await {
					Ok((datagram_size, _)) => {
						let now = SystemTime::now()
						.duration_since(time::UNIX_EPOCH)
						.expect("time is running backwards")
						.as_micros() as u64;
						
						if datagram_size == frame_buffer.len() {
							frame_buffer.resize(frame_buffer.len() * 2, 0);
							continue;
						}

						for &target in targets.iter() {
							let _ = socket.try_send_to(&frame_buffer[..datagram_size], target);
						}

						let tx = &strong_self.frame_sender;

						if tx.receiver_count() > 0 {
							tx.send((now, frame_buffer[..datagram_size].to_vec())).unwrap();
						}
					},
					Err(error) => {
						// Windows throws this error when the buffer is not large enough, while UNIX systems log whatever they can
						if error.raw_os_error() == Some(10040) {
							frame_buffer.resize(frame_buffer.len() * 2, 0);
						}
					}
				}
			}

			Ok(())
		}
	}
}

/// Periodically prunes dead forwarding targets from the database.
///
/// This function takes in a `&Arc<Mutex<SqlConnection>>` which is then downgraded to a weak
/// reference to the database. The returned Future loops until the database is dropped, at which
/// point it stops execution. This function is not intended to be used with `.await`, as it will
/// cause the current context to freeze.
/// 
/// # Improper Usage
/// 
/// This function is not intended to be used with `.await`, as it will cause the current context
/// to freeze until the database is dropped. Additionally, if a reference to the database is held
/// in the same context as the returned Future is executing, the program will not halt, as the database
/// will never be dropped since its strong reference count will never reach zero.
/// 
/// # Example
/// 
/// ```
/// // Prunes dead targets in the background every 10 seconds
/// tokio::spawn(forwarding::prune_dead_targets(&database, Duration::from_secs(10)));
/// ```
/// 
pub fn prune_dead_targets(database: &Database, period: Duration) -> impl Future<Output = ()> {
	let weak_database = Arc::downgrade(database);

	async move {
		let mut prune_interval = tokio::time::interval(period);
		prune_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

		while let Some(database) = weak_database.upgrade() {
			let timestamp = SystemTime::now()
				.duration_since(time::UNIX_EPOCH)
				.expect("time is running backwards")
				.as_secs();
			
			database
				.lock()
				.await
				.execute("DELETE FROM ForwardingTargets WHERE expiration <= ?1", rusqlite::params![timestamp])
				.unwrap();
			
			// Drop to release both the mutex lock and Arc reference to avoid holding over the sleep
			drop(database);
			prune_interval.tick().await;
		}
	}
}
