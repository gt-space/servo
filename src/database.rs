use anyhow::anyhow;
use crate::flight::FlightComputer;
use include_dir::{include_dir, Dir};
use rusqlite::Connection as SqlConnection;
use std::{sync::Arc, future::Future};
use tokio::sync::Mutex;

// include_dir is a separate library which evidently accesses files relative to
// the project root, while include_str is a standard library macro which accesses
// relative to the current file. why the difference? who knows.
const MIGRATIONS: Dir = include_dir!("./src/migrations");
const BOOTSTRAP_QUERY: &'static str = include_str!("./migrations/bootstrap.sql");

/// A convenience type representing a `rusqlite::Connection` that may be passed to multiple async
/// contexts at once.
#[derive(Clone, Debug)]
pub struct Database {
	connection: Arc<Mutex<SqlConnection>>
}

impl Database {
	/// Constructs a new `Database` enclosing a raw SQL connection.
	pub fn new(connection: SqlConnection) -> Self {
		Database {
			connection: Arc::new(Mutex::new(connection))
		}
	}

	/// Getter method which returns a reference to the enclosed conneciton.
	pub fn connection(&self) -> &Arc<Mutex<SqlConnection>> {
		&self.connection
	}

	/// Migrates the database to the latest available migration version.
	pub async fn migrate(&self) -> anyhow::Result<()> {
		let latest_migration = MIGRATIONS
			.dirs()
			.filter_map(|directory| {
				directory
					.path()
					.file_name()
					.and_then(|name| {
						name
							.to_string_lossy()
							.parse::<i32>()
							.ok()
					})
			})
			.max();
	
		if let Some(latest_migration) = latest_migration {
			self.migrate_to(latest_migration).await
		} else {
			Ok(())
		}
	}

	/// Migrates the database to a specific migration index.
	pub async fn migrate_to(&self, target_migration: i32) -> anyhow::Result<()> {
		let connection = self.connection
			.lock()
			.await;
	
		// the bootstrap query ensures that migration is set up
		// and changes nothing if it is already set up
		connection.execute_batch(BOOTSTRAP_QUERY)?;

		let current_migration = connection.query_row("SELECT MAX(migration_id) FROM Migrations", [], |row| Ok(row.get::<_, i32>(0)?))?;
	
		if current_migration < target_migration {
			for migration in current_migration + 1..=target_migration {
				let sql = MIGRATIONS
					.get_file(format!("{migration}/up.sql"))
					.ok_or(anyhow!("up.sql script for migration {migration} not found"))?
					.contents_utf8()
					.ok_or(anyhow!("up.sql script for migration {migration} could not be interpreted as UTF-8"))?;
		
				connection.execute_batch(sql)?;
				connection.execute("INSERT INTO Migrations (migration_id) VALUES (?1)", [migration])?;
			}
		} else if target_migration < current_migration {
			for migration in (target_migration..=current_migration).rev() {
				let sql = MIGRATIONS
					.get_file(format!("{migration}/down.sql"))
					.ok_or(anyhow!("down.sql script for migration {migration} not found"))?
					.contents_utf8()
					.ok_or(anyhow!("down.sql script for migration {migration} could not be interpreted as UTF-8"))?;
	
				connection.execute_batch(sql)?;
				connection.execute("DELETE FROM Migrations WHERE migration_id = ?1", [migration])?;
			}
		}
	
		Ok(())
	}

	/// Continuously logs the vehicle state every time it is notified as having changed.
	pub fn log_vehicle_state(&self, flight_computer: &FlightComputer) -> impl Future<Output = ()> {
		let connection = Arc::downgrade(&self.connection);
		let vehicle_state = flight_computer.vehicle_state();

		async move {
			loop {
				vehicle_state.1.notified().await;

				if let Some(connection) = connection.upgrade() {
					let vehicle_state = vehicle_state.0
						.lock()
						.await;

					if let Ok(serialized) = postcard::to_allocvec(&*vehicle_state) {
						connection
							.lock()
							.await
							.execute("INSERT INTO VehicleSnapshots (vehicle_state) VALUES (?1)", [serialized])
							.expect("failed to write vehicle state to database");
					}
				} else {
					break;
				}
			}
		}
	}
}
