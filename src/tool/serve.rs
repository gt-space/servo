use actix_cors::Cors;
use actix_web::{web::{self, Data}, App, HttpServer};
use rusqlite::Connection as SqlConnection;
use std::path::Path;

use crate::{
	extractors::HostMap,
	flight::FlightComputer,
	middleware::LoggingFactory,
	routes,
	Database,
};

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub async fn serve(servo_dir: &Path) -> anyhow::Result<()> {
	let sql_connection = SqlConnection::open(servo_dir.join("database.sqlite"))?;
	let flight_computer = FlightComputer::new();
	let host_map = HostMap::new();

	let database = Database::new(sql_connection);
	database.migrate().await?;

	tokio::spawn(flight_computer.auto_connect());
	tokio::spawn(database.log_vehicle_state(&flight_computer));
	tokio::spawn(flight_computer.receive_vehicle_state());

	tokio::spawn(crate::interface::display(flight_computer.vehicle_state()));

	HttpServer::new(move || {
		let cors = Cors::default()
			.allow_any_header()
			.allow_any_method()
			.allow_any_origin()
			.supports_credentials();

		App::new()
			.wrap(cors)
			.wrap(LoggingFactory::new(&database))
			.app_data(Data::new(database.clone()))
			.app_data(Data::new(flight_computer.clone()))
			.app_data(Data::new(host_map.clone()))
			.route("/data/forward", web::post().to(routes::data::start_forwarding))
			.route("/data/renew-forward", web::post().to(routes::data::renew_forwarding))
			.route("/data/export", web::post().to(routes::data::export))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
			.route("/operator/command", web::post().to(routes::command::dispatch_operator_command))
			.route("/operator/mappings", web::get().to(routes::mappings::get_mappings))
			.route("/operator/mappings", web::post().to(routes::mappings::post_mappings))
			.route("/operator/mappings", web::put().to(routes::mappings::put_mappings))
			.route("/operator/active-configuration", web::get().to(routes::mappings::get_active_configuration))
			.route("/operator/active-configuration", web::post().to(routes::mappings::activate_configuration))
			.route("/operator/sequence", web::post().to(routes::sequence::run_sequence))
	}).bind(("0.0.0.0", 7200))?
		.run()
		.await?;

	Ok(())
}
