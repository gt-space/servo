use actix_web::{App, HttpServer, web::{self, Data}};
use rusqlite::{Connection as SqlConnection, functions::FunctionFlags};
use servo::{forwarding::{self, ForwardingAgent}, middleware, routes};
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
		fs::create_dir(&servo_dir).unwrap();
	}

	let database = SqlConnection::open(servo_dir.join("database.sqlite"))?;
	let forwarding_agent = Arc::new(ForwardingAgent::new());

	database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, forwarding_agent.update_targets())?;

	database.execute_batch(include_str!("./database_schema.sql"))?;
	let database = Arc::new(Mutex::new(database));

	tokio::spawn(forwarding_agent.forward());
	tokio::spawn(forwarding_agent.log_frames(&database));
	tokio::spawn(forwarding::prune_dead_targets(&database, Duration::from_secs(10)));
	tokio::spawn(routes::auth::prune_sessions(&database, Duration::from_secs(60)));

	HttpServer::new(move || {
		App::new()
			.wrap(middleware::LoggingFactory::new(&database))
			.app_data(Data::new(database.clone()))
			.route("/auth", web::post().to(routes::auth::authenticate_user))
			.route("/data/forward", web::post().to(routes::data::start_forwarding))
			.route("/data/renew-forward", web::post().to(routes::data::renew_forwarding))
			.route("/admin/create-user", web::post().to(routes::admin::create_user))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
	}).bind(("0.0.0.0", 7200))?
		.run()
		.await?;

	Ok(())
}
