use tokio::{sync::Mutex, io::{self, AsyncWriteExt}, net::TcpStream};
use std::{future::Future, sync::Arc, net::{Ipv4Addr, SocketAddr, SocketAddrV4}, time::Duration};

/// Struct capable of performing thread-safe operations on a flight computer
/// connection, thus capable of being passed to route handlers.
#[derive(Clone, Debug)]
pub struct FlightComputer {
	connection: Arc<Mutex<Option<TcpStream>>>
}

impl FlightComputer {
	/// Constructs a thread-safe wrapper around a TCP stream which can be
	/// connected to the flight computer.
	pub fn new() -> Self {
		FlightComputer { connection: Arc::new(Mutex::new(None)) }
	}

	/// A getter which determines whether the flight computer is connected.
	pub async fn is_connected(&self) -> bool {
		return self.connection
			.lock()
			.await
			.is_some();
	}

	/// A listener function which auto-connects to the flight computer.
	/// 
	/// The flight computer is expected to fetch the IP address of the
	/// ground computer by hostname resolution, outside the scope of servo.
	pub fn auto_connect(&self) -> impl Future<Output = io::Result<()>> {
		let connection = self.connection.clone();

		async {
			let flight_listener = std::net::TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5025)))?;
			flight_listener.set_nonblocking(true)?;

			let flight_stream = loop {
				if let Ok((stream, _)) = flight_listener.accept() {
					break stream;
				}

				tokio::time::sleep(Duration::from_millis(500)).await;
			};

			*connection.lock_owned().await = Some(TcpStream::from_std(flight_stream)?);
			println!("connected to the flight computer");
			Ok(())
		}
	}

	/// Send a slice of bytes along the TCP connection to the flight computer.
	pub async fn send_bytes(&self, bytes: &[u8]) -> io::Result<()> {
		self.connection
			.lock()
			.await
			.as_mut()
			.ok_or(io::Error::new(io::ErrorKind::NotConnected, "flight computer not connected"))?
			.write_all(bytes)
			.await?;

		Ok(())
	}
}