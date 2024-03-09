use common::comm::CompositeValveState;
use crate::server::SharedState;
use std::{time::Duration, ops::Div, io::{self, Write}};
use sysinfo::{System, SystemExt, CpuExt};


struct Terminal;

impl Terminal {
	pub fn clear() {
		print!("\x1b[2J\x1b[?25l");
	}

	pub fn draw(item: &impl Drawable) {
		item.draw();
		_ = io::stdout().flush();
	}
}

trait Drawable {
	fn draw(&self);
}

struct Container {
	pub x: usize,
	pub y: usize,
	pub width: usize,
	pub height: usize,
	pub name: &'static str,
}

impl Drawable for Container {
	fn draw(&self) {
		print!("\x1b[{};{}f", self.y + 1, self.x + 1);
		print!("┌─{:─<1$}─┐", self.name, self.width - 4);

		for r in (self.y + 2)..(self.y + self.height) {
			print!("\x1b[{};{}f│{: <3$}│", r, self.x + 1, "", self.width - 2);
		}

		print!("\x1b[{};{}f└{:─<3$}┘", self.y + self.height, self.x + 1, "", self.width - 2);
	}
}

impl Container {
	pub fn write_line(&self, line: usize, message: &str) {
		print!("\x1b[{};{}f{message}", self.y + line + 2, self.x + 3);
		io::stdout().flush().unwrap();
	}
}

/// Continuously refreshes and updates the display with new data.
pub async fn display(shared: SharedState) {
	Terminal::clear();
	print!("\x1b[999;1f");

	let mut system = System::new_all();
	let system_container = Container {
		x: 0,
		y: 0,
		width: 25,
		height: 5,
		name: "System",
	};

	let mut sensors_container = Container {
		x: 25,
		y: 0,
		width: 25,
		height: 2,
		name: "Sensors",
	};

	let mut valves_container = Container {
		x: 50,
		y: 0,
		width: 25,
		height: 2,
		name: "Valves",
	};

	let hostname = system.host_name()
		.unwrap_or("\x1b[33mnone\x1b[0m".to_owned());

	loop {
		// save cursor position
		print!("\x1b[s");

		// display system statistics
		system.refresh_cpu();
		system.refresh_memory();


		let cpu_usage = system
			.cpus()
			.iter()
			.fold(0.0, |util, cpu| util + cpu.cpu_usage())
			.div(system.cpus().len() as f32);

		let memory_usage = system.used_memory() as f32 / system.total_memory() as f32 * 100.0;

		Terminal::draw(&system_container);
		system_container.write_line(0, &format!("CPU Usage: \x1b[1m{cpu_usage:.1}%\x1b[0m"));
		system_container.write_line(1, &format!("Memory Usage: \x1b[1m{memory_usage:.1}%\x1b[0m"));
		system_container.write_line(2, &format!("Host name: \x1b[1m{hostname}\x1b[0m"));
		
		// display network statistics

		// display sensor data
		let vehicle_state = shared.vehicle.0
			.lock()
			.await
			.clone();

		sensors_container.height = vehicle_state.sensor_readings.len() + 2;
		Terminal::draw(&sensors_container);

		let mut sensor_readings = vehicle_state.sensor_readings
			.iter()
			.collect::<Vec<_>>();

		sensor_readings.sort_by(|a, b| a.0.cmp(b.0));

		for (i, (name, value)) in sensor_readings.iter().enumerate() {
			sensors_container.write_line(i, &format!("{name}: {value}"));
		}

		// display valve states
		valves_container.height = vehicle_state.valve_states.len() + 2;
		Terminal::draw(&valves_container);

		for (i, (name, CompositeValveState { commanded, actual })) in vehicle_state.valve_states.iter().enumerate() {
			valves_container.write_line(i, &format!("{name}: {actual} ({commanded})"));
		}

		// restore cursor position
		print!("\x1b[u");

		io::stdout().flush().unwrap();
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
}
