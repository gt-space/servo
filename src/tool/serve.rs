use actix_cors::Cors;
use actix_web::{web::{self, Data}, App, HttpServer};
use std::path::Path;

use crate::{
	extractors::HostMap,
	flight::FlightComputer,
	forwarding::ForwardingAgent,
	middleware::LoggingFactory,
	routes,
	Database,
};

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub async fn serve(servo_dir: &Path) -> anyhow::Result<()> {
	let database = Database::open(&servo_dir.join("database.sqlite"))?;
	let flight_computer = FlightComputer::new(&database);
	let host_map = HostMap::new();

	let forwarding_agent = ForwardingAgent::new(flight_computer.vehicle_state());

	database.migrate().await?;

	tokio::spawn(flight_computer.auto_connect());
	tokio::spawn(database.log_vehicle_state(&flight_computer));
	tokio::spawn(flight_computer.receive_vehicle_state());
	tokio::spawn(forwarding_agent.forward());

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
			.app_data(Data::new(forwarding_agent.clone()))
			.route("/data/forward", web::get().to(routes::data::forward))
			.route("/data/export", web::post().to(routes::data::export))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
			.route("/operator/command", web::post().to(routes::command::dispatch_operator_command))
			.route("/operator/mappings", web::get().to(routes::mappings::get_mappings))
			.route("/operator/mappings", web::post().to(routes::mappings::post_mappings))
			.route("/operator/mappings", web::put().to(routes::mappings::put_mappings))
			.route("/operator/mappings", web::delete().to(routes::mappings::delete_mappings))
			.route("/operator/active-configuration", web::get().to(routes::mappings::get_active_configuration))
			.route("/operator/active-configuration", web::post().to(routes::mappings::activate_configuration))
			.route("/operator/sequence", web::get().to(routes::sequence::retrieve_sequences))
			.route("/operator/sequence", web::put().to(routes::sequence::save_sequence))
			.route("/operator/sequence", web::delete().to(routes::sequence::delete_sequence))
			.route("/operator/run-sequence", web::post().to(routes::sequence::run_sequence))
	}).bind(("0.0.0.0", 7200))?
		.run()
		.await?;

	Ok(())
}
