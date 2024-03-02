use clap::ArgMatches;
use common::comm::{Measurement, Unit, ValveState, VehicleState};
use std::{net::{TcpStream, UdpSocket}, thread, time::Duration};

pub fn emulate_flight() -> anyhow::Result<()> {
	Ok(())
}

pub fn emulate_sam() -> anyhow::Result<()> {
	Ok(())
}

/// Tool function which emulates different components of the software stack.
pub fn emulate(args: &ArgMatches) -> anyhow::Result<()> {
	let _flight = TcpStream::connect("localhost:5025")?;

	let data_socket = UdpSocket::bind("0.0.0.0:0")?;
	data_socket.connect("localhost:7201")?;

	let mut mock_vehicle_state = VehicleState::new();
	mock_vehicle_state.valve_states.insert("BBV".to_owned(), ValveState::Closed);
	mock_vehicle_state.valve_states.insert("SWV".to_owned(), ValveState::CommandedClosed);
 
	let mut raw = postcard::to_allocvec(&mock_vehicle_state)?;
	postcard::from_bytes::<VehicleState>(&raw).unwrap();

	loop {
		mock_vehicle_state.sensor_readings.insert("KBPT".to_owned(), Measurement { value: rand::random::<f64>() * 120.0, unit: Unit::Psi });
		mock_vehicle_state.sensor_readings.insert("WTPT".to_owned(), Measurement { value: rand::random::<f64>() * 1000.0, unit: Unit::Psi });
		mock_vehicle_state.sensor_readings.insert("BBV_V".to_owned(), Measurement { value: rand::random::<f64>() * 24.0, unit: Unit::Volts });
		mock_vehicle_state.sensor_readings.insert("BBV_I".to_owned(), Measurement { value: rand::random::<f64>() * 0.1, unit: Unit::Amps });
		mock_vehicle_state.valve_states.insert("BBV".to_owned(), ValveState::Closed);
		mock_vehicle_state.valve_states.insert("OMV".to_owned(), ValveState::Open);
		raw = postcard::to_allocvec(&mock_vehicle_state)?;

		data_socket.send(&raw)?;
		thread::sleep(Duration::from_millis(10));
	}
}
