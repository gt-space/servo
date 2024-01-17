use common::{ControlMessage, NodeMapping, VehicleState, Sequence};
use crate::Database;
use tokio::{sync::{Mutex, Notify}, io::{self, AsyncWriteExt}, net::{TcpStream, UdpSocket}};

use std::{
	future::Future,
	net::{Ipv4Addr, SocketAddr, SocketAddrV4},
	sync::Arc,
	time::Duration,
};

/// Struct capable of performing thread-safe operations on a flight computer
/// connection, thus capable of being passed to route handlers.
#[derive(Clone, Debug)]
pub struct FlightComputer {
	connection: Arc<Mutex<Option<TcpStream>>>,
	vehicle_state: Arc<(Mutex<VehicleState>, Notify)>,
	database: Database,
}

impl FlightComputer {
	/// Constructs a thread-safe wrapper around a TCP stream which can be
	/// connected to the flight computer.
	pub fn new(database: &Database) -> Self {
		FlightComputer {
			connection: Arc::new(Mutex::new(None)),
			vehicle_state: Arc::new((Mutex::new(VehicleState::new()), Notify::new())),
			database: database.clone()
		}
	}

	/// A getter that creates a new weak reference to the rocket state.
	pub fn vehicle_state(&self) -> Arc<(Mutex<VehicleState>, Notify)> {
		self.vehicle_state.clone()
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

	/// Sends the given set of mappings to the flight computer.
	pub async fn send_mappings(&self) -> anyhow::Result<()> {
		let mappings = self.database
			.connection()
			.lock()
			.await
			.prepare("SELECT text_id, board_id, channel_type, channel, computer FROM NodeMappings WHERE active = TRUE")?
			.query_and_then([], |row| {
				Ok(NodeMapping {
					text_id: row.get(0)?,
					board_id: row.get(1)?,
					channel_type: row.get(2)?,
					channel: row.get(3)?,
					computer: row.get(4)?,
				})
			})?
			.collect::<Result<Vec<NodeMapping>, rusqlite::Error>>()?;

		let message = ControlMessage::Mappings(mappings);
		let serialized = postcard::to_allocvec(&message)?;

		self.send_bytes(&serialized).await?;

		Ok(())
	}

	/// Sends the given sequence to the flight computer to be executed.
	pub async fn send_sequence(&self, name: &str, script: &str) -> anyhow::Result<()> {
		let sequence = Sequence {
			name: name.to_owned(),
			script: script.to_owned(),
		};

		let message = ControlMessage::Sequence(sequence);
		let serialized = postcard::to_allocvec(&message)?;

		self.send_bytes(&serialized).await?;
		Ok(())
	}

	/// Repeatedly receives vehicle state information from the flight computer.
	pub fn receive_vehicle_state(&self) -> impl Future<Output = io::Result<()>> {
		let weak_vehicle_state = Arc::downgrade(&self.vehicle_state);

		async move {
			let socket = UdpSocket::bind("0.0.0.0:7201").await.unwrap();
			let mut frame_buffer = vec![0; 521];

			while let Some(vehicle_state) = weak_vehicle_state.upgrade() {
				match socket.recv_from(&mut frame_buffer).await {
					Ok((datagram_size, _)) => {
						if datagram_size == frame_buffer.len() {
							frame_buffer.resize(frame_buffer.len() * 2, 0);
							continue;
						}

						if let Ok(new_state) = postcard::from_bytes::<VehicleState>(&frame_buffer[..datagram_size]) {
							*vehicle_state.0.lock().await = new_state;
							vehicle_state.1.notify_waiters();
						}
					},
					Err(error) => {
						// Windows throws this error when the buffer is not large enough.
						// Unix systems just log whatever they can.
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