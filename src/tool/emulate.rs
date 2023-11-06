use tokio::net::UdpSocket;

use common::{VehicleState, Unit, ValveState};

pub async fn emulate() -> anyhow::Result<()> {
	let data_socket = UdpSocket::bind("0.0.0.0:0").await?;
	println!("{:?}", data_socket.local_addr());
	data_socket.connect("localhost:7201").await?;

	let mut mock_vehicle_state = VehicleState::new();
	mock_vehicle_state.sensor_readings.insert("KBPT".to_owned(), Unit::Psi(120.0));
	mock_vehicle_state.sensor_readings.insert("WTPT".to_owned(), Unit::Psi(1000.0));
	mock_vehicle_state.valve_states.insert("BBV".to_owned(), ValveState::Closed);
	mock_vehicle_state.valve_states.insert("SWV".to_owned(), ValveState::CommandedClosed);

	let raw = postcard::to_allocvec(&mock_vehicle_state)?;

	data_socket.send(&raw).await?;

	Ok(())
}
