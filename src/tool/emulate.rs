use tokio::net::UdpSocket;

use common::comm::{Measurement, Unit, ValveState, VehicleState};

pub async fn emulate() -> anyhow::Result<()> {
	let data_socket = UdpSocket::bind("0.0.0.0:0").await?;
	println!("{:?}", data_socket.local_addr());
	data_socket.connect("localhost:7201").await?;

	let mut mock_vehicle_state = VehicleState::new();
	mock_vehicle_state.valve_states.insert("BBV".to_owned(), ValveState::Closed);
	mock_vehicle_state.valve_states.insert("SWV".to_owned(), ValveState::CommandedClosed);
 
	let raw = postcard::to_allocvec(&mock_vehicle_state)?;
	postcard::from_bytes::<VehicleState>(&raw).unwrap();

	loop {
		mock_vehicle_state.sensor_readings.insert("KBPT".to_owned(), Measurement { value: rand::random::<f64>() * 120.0, unit: Unit::Psi });
		mock_vehicle_state.sensor_readings.insert("WTPT".to_owned(), Measurement { value: rand::random::<f64>() * 1000.0, unit: Unit::Psi });

		data_socket.send(&raw).await?;
	}
}
