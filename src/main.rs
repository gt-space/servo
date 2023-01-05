mod ctrl;
mod extractors;
mod forwarding;
mod middleware;
mod protocol;
mod routes;

use actix_web::{App, HttpServer, web::{self, Data}};
use forwarding::ForwardingAgent;
use rusqlite::{Connection as SqlConnection, functions::FunctionFlags};
use std::{env, fs, path::Path, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub type Database = Arc<Mutex<rusqlite::Connection>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let hitl_dir = Path::new(&env::var("HOME")?)
		.join(".hitl");

	if !hitl_dir.is_dir() {
		fs::create_dir(&hitl_dir).unwrap();
	}

	let database = SqlConnection::open(hitl_dir.join("database.sqlite"))?;
	let forwarding_agent = Arc::new(ForwardingAgent::new());

	database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, forwarding_agent.update_targets())?;

	database.execute_batch(include_str!("./database_schema.sql"))?;
	let database = Arc::new(Mutex::new(database));

	tokio::spawn(forwarding_agent.forward());
	tokio::spawn(forwarding::prune_dead_targets(&database, Duration::from_secs(10)));

	HttpServer::new(move || {
		App::new()
			.wrap(middleware::LoggingFactory::new(&database))
			.app_data(Data::new(database.clone()))
			.route("/auth", web::post().to(routes::auth::authenticate_user))
			.route("/meta/logs", web::get().to(routes::meta::get_logs))
			.route("/data/forward", web::post().to(routes::data::start_forwarding))
			.route("/data/renew-forward", web::post().to(routes::data::renew_forwarding))
			.route("/hitl/test", web::post().to(routes::hitl::test_procedure))
			.route("/admin/create-user", web::post().to(routes::admin::create_user))
			.route("/admin/sql", web::post().to(routes::admin::execute_sql))
	}).bind(("127.0.0.1", 7200))?
		.run()
		.await?;

	Ok(())
}
