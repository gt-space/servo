use std::{
	collections::HashSet,
	future::Future,
	io,
	net::SocketAddr,
	sync::{Arc, Mutex},
};

use tokio::{net::UdpSocket, time::{Duration, Instant}};

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

	pub fn remove_target(&self, address: SocketAddr) {
		self.targets
			.lock()
			.unwrap()
			.remove(&address);
	}

	pub fn forward(self: &Arc<Self>) -> impl Future<Output = io::Result<()>> {
		let weak_self = Arc::downgrade(self);

		async move {
			let socket = UdpSocket::bind("127.0.0.1:7201").await?;
			let mut buffer = [0_u8; 512];

			while let Some(strong_self) = weak_self.upgrade() {
				// 15ms frames is ~60 updates / sec; adjust if necessary
				let mut end_frame = Instant::now() + Duration::from_millis(15);

				'forward_frame: { // Block to constrain the lifetime of 'targets' so that it doesn't cross an await
					let targets = strong_self.targets.lock().unwrap();

					if targets.len() == 0 {
						end_frame += Duration::from_millis(500);
						break 'forward_frame;
					}

					if let Ok((frame_size, _)) = socket.try_recv_from(&mut buffer) {
						for &target in targets.iter() {
							let _ = socket.try_send_to(&buffer[..frame_size], target);
						}
					}
				}

				let wait_duration = end_frame.duration_since(Instant::now());

				// Wait until the end of the frame
				tokio::time::sleep(wait_duration).await;
			}

			Ok(())
		}
	}
}
