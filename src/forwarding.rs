use std::{
	collections::HashSet,
	io,
	net::{SocketAddr, UdpSocket},
	sync::{Arc, Mutex},
	thread,
	time::{Duration, Instant}
};

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
			.expect("targets mutex is poisoned")
			.insert(address);
	}

	pub fn remove_target(&self, address: SocketAddr) {
		self.targets
			.lock()
			.expect("targets mutex is poisoned")
			.remove(&address);
	}

	pub fn forward(self: &Arc<Self>) -> io::Result<()> {
		let socket = UdpSocket::bind("127.0.0.1:7201")?;

		// This obtains the datagram size and allocates a buffer. It assumes:
		// 1) The max size of the UDP datagram data is 512 bytes
		// 2) Every datagram from the source is the same size
		let mut temp_buffer = [0_u8; 512];
		let (buffer_size, source_address) = socket.recv_from(&mut temp_buffer)?;
		let mut buffer = vec![0; buffer_size];
		drop(temp_buffer);

		let weak_self = Arc::downgrade(self);

		socket.set_nonblocking(true)?;

		println!("buffer size: {buffer_size}");

		thread::spawn(move || {
			while let Some(strong_self) = weak_self.upgrade() {
				// 15ms is ~60 updates / sec; adjust if necessary
				let end_frame = Instant::now() + Duration::from_millis(15);
				let targets = strong_self.targets.lock().unwrap();

				if targets.len() == 0 {
					drop(targets);

					println!("waiting....");
					thread::sleep(Duration::from_millis(500));
					continue;
				}

				while socket.recv_from(&mut buffer).unwrap().1 != source_address {}

				for &target in targets.iter() {
					socket.send_to(&buffer, target).unwrap();
				}

				// Release the mutex lock before waiting so other threads can access it
				drop(targets);

				// Wait until the end of the frame
				thread::sleep(end_frame.duration_since(Instant::now()));
			}
		});

		Ok(())
	}
}

// --- NOTE ---
// To developers of the future (likely myself) looking at this file and thinking,
// "Well, the web server is done async, but this is done multithreaded.
// It even uses two different types of Mutex (std::sync::Mutex and tokio::sync::Mutex)
// in the same application, one for the database and one for the forwarding agent's
// internal data. Why not just rewrite ForwardingAgent to use async with tokio?"
// Standalone, that was how ForwardingAgent was originally written, so that tokio
// could dynamically determine how many threads were necessary to optimize parallel
// running of both the server and the agents. Alas, rusqlite's 'create_scalar_function'
// database method (which must access a ForwardingAgent) is not written to be
// compatible with async but requires of every captured variable that it is thread-safe,
// implicitly requiring that every captured Mutex is capable of being poisoned if a
// thread panics while holding the lock. Guess what? Only std::sync::Mutex implements this.
// If trying to use tokio::sync::Mutex (even behind an Arc) within the closure of the
// 'create_scalar_function' method, the borrow checker will throw a cryptic error about
// an UnsafeCell _possibly_ containing interior mutability which prevents it from being
// moved to the closure. Do not be fooled. The sirens want to lead you off into the depths
// of the Rust documentation. UnsafeCell is used in the underlying data structure behind
// the tokio Mutex, making it not thread-safe. Cloning behind an Arc doesn't work and
// neither does std::sync::Mutex as they both create lifetime issues for the borrow checker.
// I would love to revisit this or help anyone who would like to attempt. I bet I can
// find an async solution with a bit more work, but it is likely to be the exact same speed
// as this one. This solution works very well. Some things appear to be simplifiable. They
// are almost certainly not. If you have a question about why I used Arc here or Mutex there
// or UdpSocket instead of TcpSocket, I'd be happy to explain. Just reach out to me.

// - Jeff
