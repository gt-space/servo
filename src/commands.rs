use actix_web::{App, HttpServer, web::{self, Data}};
use actix_cors::Cors;
use rusqlite::{Connection as SqlConnection, functions::FunctionFlags};
use crate::{forwarding::{self, ForwardingAgent}, middleware, routes, flight::FlightComputer};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::Mutex;

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub async fn serve(servo_dir: &Path) -> anyhow::Result<()> {
	let database = SqlConnection::open(servo_dir.join("database.sqlite"))?;
	let forwarding_agent = Arc::new(ForwardingAgent::new());
	let flight_computer = FlightComputer::new();

	database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, forwarding_agent.update_targets())?;

	database.execute_batch(include_str!("./database_schema.sql"))?;
	let database = Arc::new(Mutex::new(database));

	tokio::spawn(flight_computer.auto_connect());
	tokio::spawn(forwarding_agent.forward());
	tokio::spawn(forwarding_agent.log_frames(&database));
	tokio::spawn(forwarding::prune_dead_targets(&database, Duration::from_secs(10)));
	tokio::spawn(routes::auth::prune_sessions(&database, Duration::from_secs(60)));

	HttpServer::new(move || {
		let cors = Cors::default()
			.allow_any_header()
			.allow_any_method()
			.allow_any_origin()
			.supports_credentials();

		App::new()
			.wrap(cors)
			.wrap(middleware::LoggingFactory::new(&database))
			.app_data(Data::new(database.clone()))
			.app_data(Data::new(flight_computer.clone()))
			.route("/auth", web::post().to(routes::auth::authenticate_user))
			.route("/data/forward", web::post().to(routes::data::start_forwarding))
			.route("/data/renew-forward", web::post().to(routes::data::renew_forwarding))
			.route("/admin/create-user", web::post().to(routes::admin::create_user))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
			.route("/operator/command", web::post().to(routes::command::dispatch_operator_command))
			.route("/operator/mappings", web::get().to(routes::mappings::get_mappings))
			.route("/operator/mappings", web::post().to(routes::mappings::set_mappings))
	}).bind(("0.0.0.0", 7200))?
		.run()
		.await?;
	Ok(())
}