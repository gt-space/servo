use std::{collections::HashSet, io, net::{SocketAddr, UdpSocket}, sync::{Arc, Mutex, Weak}, thread};

#[derive(Clone)]
pub struct ForwardingAgent {
	source: SocketAddr,
	targets: Arc<Mutex<HashSet<SocketAddr>>>
}

impl ForwardingAgent {
	pub fn new(source: SocketAddr) -> io::Result<ForwardingAgent> {
		Ok(ForwardingAgent { source, targets: Arc::new(Mutex::new(HashSet::new())) })
	}

	pub fn add_target(&mut self, target: SocketAddr) {
		let mut targets = self.targets
			.lock()
			.expect("forwarding data mutex is poisoned");

		if targets.insert(target) && targets.len() == 1 {
			let thread_source = self.source.clone();
			let thread_targets = Arc::downgrade(&self.targets);

			thread::spawn(move || {
				ForwardingAgent::forward(thread_source, thread_targets).expect("forwarding failed");
			});
		}
	}

	pub fn remove_target(&mut self, target: &SocketAddr) {
		self.targets
			.lock()
			.expect("forwarding data mutex is poisoned")
			.remove(&target);
	}

	fn forward(source: SocketAddr, targets: Weak<Mutex<HashSet<SocketAddr>>>) -> io::Result<()> {
		let socket = UdpSocket::bind("127.0.0.1:0")?;
		socket.connect(source)?;

		// This obtains the datagram size and allocates a buffer. It assumes:
		// 1) The max size of the UDP datagram data is 512 bytes
		// 2) Every datagram from the source is the same size
		let mut temp_buffer = [0_u8; 512];
		let buffer_size = socket.recv(&mut temp_buffer)?;
		let mut buffer = Vec::with_capacity(buffer_size);

		while let Some(targets) = targets.upgrade() {
			let targets = targets
				.lock()
				.expect("forwarding data mutex is poisoned");

			if targets.len() == 0 {
				break;
			}

			for _ in 0..100 {
				socket.recv(&mut buffer).expect("socket not connected");

				for target in targets.iter() {
					let _ = socket.send_to(&buffer, target);
				}
			}
		}

		Ok(())
	}
}
