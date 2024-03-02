use axum::{routing::{delete, get, post, put}, Router};
use tower_http::cors::{self, CorsLayer};
use crate::{interface, server::{routes, Database, FlightComputer, SharedState}};
use std::{io, net::SocketAddr, path::Path};
use tokio::net::TcpListener;

/// Performs the necessary setup to connect to the servo server.
/// This function initializes database connections, spawns background tasks,
/// and starts the HTTP server to serve the application upon request.
pub fn serve(servo_dir: &Path) -> anyhow::Result<()> {
	let database = Database::open(&servo_dir.join("database.sqlite"))?;
	database.migrate()?;

	tokio::runtime::Builder::new_multi_thread()
		.worker_threads(10)
		.enable_all()
		.build()
		.unwrap()
		.block_on(async move {
			let shared_state = SharedState::new(database);

			tokio::spawn(FlightComputer::auto_connect(&shared_state));
			tokio::spawn(shared_state.database.log_vehicle_state(&shared_state));
			tokio::spawn(interface::display(shared_state.clone()));

			let cors = CorsLayer::new()
				.allow_methods(cors::Any)
				.allow_headers(cors::Any)
				.allow_origin(cors::Any);

			let server = Router::new()
				.route("/data/forward", get(routes::forward_data))
				.route("/data/export", post(routes::export))
				.route("/admin/sql", post(routes::execute_sql))
				.route("/operator/command", post(routes::dispatch_operator_command))
				.route("/operator/mappings", get(routes::get_mappings))
				.route("/operator/mappings", post(routes::post_mappings))
				.route("/operator/mappings", put(routes::put_mappings))
				.route("/operator/mappings", delete(routes::delete_mappings))
				.route("/operator/active-configuration", get(routes::get_active_configuration))
				.route("/operator/active-configuration", post(routes::activate_configuration))
				.route("/operator/calibrate", post(routes::calibrate))
				.route("/operator/sequence", get(routes::retrieve_sequences))
				.route("/operator/sequence", put(routes::save_sequence))
				.route("/operator/sequence", delete(routes::delete_sequence))
				.route("/operator/run-sequence", post(routes::run_sequence))
				.layer(cors)
				.with_state(shared_state)
				.into_make_service_with_connect_info::<SocketAddr>();
			
			let listener = TcpListener::bind("0.0.0.0:7200").await?;
			axum::serve(listener, server).await?;

			io::Result::Ok(())
		})?;

	Ok(())
}
