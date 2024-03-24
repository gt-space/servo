use clap::ArgMatches;
use crate::{interface, server::{flight, Server}};
use std::path::Path;

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub fn serve(servo_dir: &Path, args: &ArgMatches) -> anyhow::Result<()> {
	let volatile = args.get_one::<bool>("volatile")
		.copied()
		.unwrap_or(false);

	let quiet = args.get_one::<bool>("quiet")
		.copied()
		.unwrap_or(false);


	let database_path = servo_dir.join("database.sqlite");
	let server = Server::new((!volatile).then_some(&database_path))?;

	server.shared.database.migrate()?;

	tokio::runtime::Builder::new_multi_thread()
		.worker_threads(10)
		.enable_all()
		.build()
		.unwrap()
		.block_on(async move {
			tokio::spawn(flight::auto_connect(&server.shared));
			tokio::spawn(server.shared.database.log_vehicle_state(&server.shared));

			if !quiet {
				tokio::spawn(interface::display(server.shared.clone()));
			}

			server.serve().await
		})?;

	Ok(())
}
