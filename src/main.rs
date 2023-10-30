use actix_web::{App, HttpServer, web::{self, Data}};
use actix_cors::Cors;
use rusqlite::{Connection as SqlConnection, functions::FunctionFlags};
use servo::{forwarding::{self, ForwardingAgent}, middleware, routes, flight::FlightComputer, extractors::HostMap};
use std::{env, fs, path::Path, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::init();

	#[cfg(target_family = "windows")]
	let home_path = &env::var("USERPROFILE")?;

	#[cfg(target_family = "unix")]
	let home_path = &env::var("HOME")?;

	let servo_dir = Path::new(home_path).join(".servo");

	if !servo_dir.is_dir() {
		fs::create_dir(&servo_dir)
			.expect("failed to create .servo directory");
	}

	let database = SqlConnection::open(servo_dir.join("database.sqlite"))?;
	let forwarding_agent = Arc::new(ForwardingAgent::new());
	let flight_computer = FlightComputer::new();
	let host_map = HostMap::new();

	database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, forwarding_agent.update_targets())?;

	database.execute_batch(include_str!("./database_schema.sql"))?;
	let database = Arc::new(Mutex::new(database));

	tokio::spawn(flight_computer.auto_connect());
	tokio::spawn(forwarding_agent.forward());
	tokio::spawn(forwarding_agent.log_frames(&database));
	tokio::spawn(forwarding::prune_dead_targets(&database, Duration::from_secs(10)));

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
			.app_data(Data::new(host_map.clone()))
			.route("/data/forward", web::post().to(routes::data::start_forwarding))
			.route("/data/renew-forward", web::post().to(routes::data::renew_forwarding))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
			.route("/operator/command", web::post().to(routes::command::dispatch_operator_command))
			.route("/operator/mappings", web::get().to(routes::mappings::get_mappings))
			.route("/operator/mappings", web::post().to(routes::mappings::set_mappings))
	}).bind(("0.0.0.0", 7200))?
		.run()
		.await?;

	Ok(())
}
