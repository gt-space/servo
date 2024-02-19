mod database;
mod error;
mod flight;

/// All server API route functions.
pub mod routes;

use common::comm::VehicleState;
pub use database::Database;
pub use error::{ServerError as Error, ServerResult as Result};
pub use flight::FlightComputer;

use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};

/// Contains all of Servo's shared server state.
#[derive(Clone, Debug)]
pub struct SharedState {
	/// The database, a wrapper over `Arc<Mutex<SqlConnection>>`, so that it may
	/// be accessed in route functions.
	pub database: Database,

	/// The option for a flight computer.
	pub flight: Arc<(Mutex<Option<FlightComputer>>, Notify)>,

	/// The option for a ground computer.
	pub ground: Arc<(Mutex<Option<FlightComputer>>, Notify)>,

	/// The state of the vehicle, including both flight and ground components.
	pub vehicle: Arc<(RwLock<VehicleState>, Notify)>,
}

impl SharedState {
	/// Creates a new shared state given a database.
	pub fn new(database: Database) -> Self {
		let vehicle_state = Arc::new((RwLock::new(VehicleState::new()), Notify::new()));

		SharedState {
			database,
			flight: Arc::new((Mutex::new(None), Notify::new())),
			ground: Arc::new((Mutex::new(None), Notify::new())),
			vehicle: vehicle_state,
		}
	}
}
