use crate::protocol::{TestDescription, TestLog};
use std::{error, fmt, thread, time::{Instant, Duration}};

pub struct TestEnv;

pub struct Outcome {
	pub start_time: Instant,
	pub end_time: Instant,
}

#[derive(Debug)]
pub enum Measurement {
	Amps(f64),
	Volts(f64),
}

impl fmt::Display for Measurement {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Amps(q) => write!(f, "{q}A"),
			Self::Volts(q) => write!(f, "{q}V"),
		}
	}
}

#[derive(Debug)]
pub enum Error {
	UnexpectedMeasurement {
		expected: Measurement,
		actual: Measurement,
	},
}

impl error::Error for Error {}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Error::UnexpectedMeasurement { expected, actual } => write!(f, "expected {expected}V but measured {actual}V")
		}
	}
}

pub fn run_test(test: &TestDescription) -> Result<Outcome, Error> {
	let start_time = Instant::now();

	for stage in &test.stages {
		for event in &stage.sequence {
			thread::sleep(start_time + Duration::from_millis(event.t as u64) - Instant::now());

			// perform actions
		}
	}

	let end_time = Instant::now();

	unimplemented!();
}
