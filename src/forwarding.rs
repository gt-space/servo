use std::{
	collections::HashSet,
	future::Future,
	io,
	net::SocketAddr,
	sync::{Arc, Mutex},
	time::{self, SystemTime},
};

use crate::Database;
use tokio::{net::UdpSocket, time::{Duration, Instant, MissedTickBehavior}};

pub struct ForwardingAgent {
	targets: Mutex<HashSet<SocketAddr>>,
}

impl ForwardingAgent {
	pub fn new() -> Self {
		ForwardingAgent { targets: Mutex::new(HashSet::new()) }
	}

	pub fn add_target(&self, address: SocketAddr) {
		self.targets
			.lock()
			.unwrap()
			.insert(address);
	}

	pub fn remove_target(&self, address: &SocketAddr) {
		self.targets
			.lock()
			.unwrap()
			.remove(address);
	}

	pub fn forward(self: &Arc<Self>) -> impl Future<Output = io::Result<()>> {
		let weak_self = Arc::downgrade(self);

		async move {
			let socket = UdpSocket::bind("127.0.0.1:7201").await?;
			let mut buffer = [0_u8; 512];

			// 15ms frames is ~60 updates / sec; adjust if necessary
			let mut frame_interval = tokio::time::interval(Duration::from_millis(15));

			while let Some(strong_self) = weak_self.upgrade() {
				// Block to constrain the lifetime of 'targets' so that it doesn't cross an await
				// (this is a requirement of std::sync::Mutex)
				'forward_frame: {
					let targets = strong_self.targets.lock().unwrap();

					if targets.len() == 0 {
						break 'forward_frame;
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
/// # Warnings
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
