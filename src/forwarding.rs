use std::{
	collections::HashSet,
	future::Future,
	io,
	net::SocketAddr,
	sync::Arc,
	time::{self, SystemTime},
};

use crate::Database;
use rusqlite::functions;
use tokio::{net::UdpSocket, sync::Mutex, time::{Duration, MissedTickBehavior}};

/// A struct which can forward messages incoming on a single UDP port to multiple external sockets 
pub struct ForwardingAgent {
	targets: Mutex<HashSet<SocketAddr>>,
	frame_duration: Duration,
}

impl ForwardingAgent {
	/// Constructs a new ForwardingAgent with a 15ms frame duration.
	pub fn new() -> Self {
		ForwardingAgent {
			targets: Mutex::new(HashSet::new()),
			frame_duration: Duration::from_millis(15),
		}
	}

	/// Returns the current duration of one fowarding frame (default 15ms).
	pub fn frame_duration(&self) -> Duration {
		return self.frame_duration.clone();
	}

	/// Sets the duration of a single forwarding frame (default 15ms).
	/// This method must be called before `ForwardingAgent::forward` to have the intended effect.
	pub fn set_frame_duration(&mut self, frame_duration: Duration) {
		self.frame_duration = frame_duration;
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
		let frame_duration = self.frame_duration;

		async move {
			let socket = UdpSocket::bind("127.0.0.1:7201").await?;
			let mut buffer = [0_u8; 512];

			// 15ms frames is ~60 updates / sec; adjust if necessary
			let mut frame_interval = tokio::time::interval(frame_duration);

			while let Some(strong_self) = weak_self.upgrade() {
				'frame: {
					let targets = strong_self.targets.lock().await;

					if targets.len() == 0 {
						break 'frame;
					}

					if let Ok((frame_size, _)) = socket.try_recv_from(&mut buffer) {
						for &target in targets.iter() {
							let _ = socket.try_send_to(&buffer[..frame_size], target);
						}
					}
				}

				// Wait until the end of the frame
				frame_interval.tick().await;
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
