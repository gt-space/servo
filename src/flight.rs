use fs_protobuf_rust::compiled::mcfs::{
	self,
	core::mod_Message::OneOfcontent,
	device::DeviceType,
	status::{Status, DeviceInfo, mod_Status::OneOfstatus},
};

use tokio::{sync::Mutex, net::UdpSocket, io::{self, AsyncWriteExt}};
use std::{future::Future, sync::Arc, borrow::Cow, net::{Ipv4Addr, SocketAddr, SocketAddrV4}, time::Duration};

use tokio::{net::TcpStream};

#[derive(Clone, Debug)]
pub struct FlightComputer {
	connection: Arc<Mutex<Option<TcpStream>>>
}

impl FlightComputer {
	pub fn new() -> Self {
		FlightComputer { connection: Arc::new(Mutex::new(None)) }
	}

	pub async fn is_connected(&self) -> bool {
		return self.connection
			.lock()
			.await
			.is_some();
	}

	pub fn auto_connect(&self) -> impl Future<Output = io::Result<()>> {
		let connection = self.connection.clone();

		async {
			let multicast_group = Ipv4Addr::new(224, 0, 0, 3);
			let multicast_socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 6000)).await.unwrap();

			multicast_socket.join_multicast_v4(Ipv4Addr::new(224, 0, 0, 3), Ipv4Addr::UNSPECIFIED)?;
			multicast_socket.set_multicast_loop_v4(false)?;

			let flight_listener = std::net::TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5025)))?;
			flight_listener.set_nonblocking(true)?;

			let discovery_message = mcfs::core::Message {
				timestamp: None,
				board_id: 150,
				content: OneOfcontent::status(
					Status {
						status_message: Cow::Borrowed("doing good"),
						status: OneOfstatus::device_info(
							DeviceInfo {
								board_id: 150,
								device_type: DeviceType::SERVER
							}
						)
					}
				)
			};

			let packet = quick_protobuf::serialize_into_vec(&discovery_message).unwrap();

			let flight_stream = loop {
				multicast_socket.send_to(&packet, (multicast_group, 6000)).await?;
				tokio::time::sleep(Duration::from_millis(500)).await;

				if let Ok((stream, _)) = flight_listener.accept() {
					break stream;
				}

				tokio::time::sleep(Duration::from_secs(9)).await;
			};

			*connection.lock_owned().await = Some(TcpStream::from_std(flight_stream)?);
			println!("connected to the flight computer");
			Ok(())
		}
	}

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