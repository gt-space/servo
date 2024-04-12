use clap::ArgMatches;
use crate::{interface, server::{flight, Server}, Cache};
use jeflog::fail;

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub fn serve(args: &ArgMatches){
	let volatile = args.get_one::<bool>("volatile")
		.copied()
		.unwrap_or(false);

	let quiet = args.get_one::<bool>("quiet")
		.copied()
		.unwrap_or(false);

	let database_path = Cache::get().path.join("database.sqlite");
	let server = match Server::new((!volatile).then_some(&database_path)) {
		Ok(server) => server,
		Err(error) => {
			fail!("Failed to construct server: {error}");
			return;
		},
	};

	if let Err(error) = server.shared.database.migrate() {
		fail!("Failed to migrate database: {error}");
		return;
	}

	let server_result = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(10)
		.enable_all()
		.build()
		.unwrap()
		.block_on(async move {
			tokio::spawn(flight::auto_connect(&server.shared));
			tokio::spawn(flight::receive_vehicle_state(&server.shared));
			tokio::spawn(server.shared.database.log_vehicle_state(&server.shared));

			if !quiet {
				tokio::spawn(interface::display(server.shared.clone()));
			}

			server.serve().await
		});

	if let Err(error) = server_result {
		fail!("Server crashed: {error}");
	}
}
