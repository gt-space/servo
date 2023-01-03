mod extractors;
mod hitl;
mod forwarding;
mod middleware;
mod protocol;
mod routes;

use actix_web::{App, HttpServer, web::{self, Data}};
use argon2::password_hash::SaltString;
use forwarding::ForwardingAgent;
use rand::rngs::OsRng;
use rusqlite::{Connection as SqlConnection, functions::FunctionFlags};
use std::{env, fs, path::Path, sync::Arc};
use tokio::sync::Mutex;

pub type ThreadedDatabase = Arc<Mutex<rusqlite::Connection>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let hitl_dir = Path::new(&env::var("HOME")?)
		.join(".hitl");

	if !hitl_dir.is_dir() {
		fs::create_dir(&hitl_dir).unwrap();
	}

	let database = SqlConnection::open(hitl_dir.join("database.sqlite"))?;
	let root_salt = SaltString::generate(&mut OsRng).to_string();

	let forwarding_agent = Arc::new(ForwardingAgent::new());

	database.create_scalar_function("forward_target", 2, FunctionFlags::SQLITE_UTF8, {
		let forwarding_agent = forwarding_agent.clone();

		move |context| {
			let target_address = context
				.get::<String>(0)?
				.parse()
				.unwrap();

			let should_add = context.get::<bool>(1)?;

			if should_add {
				forwarding_agent.add_target(target_address);
			} else {
				forwarding_agent.remove_target(target_address);
			}

			Ok(1)
		}
	})?;

	database.execute_batch(include_str!("./database_schema.sql"))?;
	database.execute("INSERT OR IGNORE INTO Users VALUES ('root', NULL, ?1, 1)", rusqlite::params![root_salt])?;
	database.execute("DELETE FROM ForwardingSessions", [])?;
	
	forwarding_agent.forward()?;

	let threaded_database = Arc::new(Mutex::new(database));

	HttpServer::new(move || {
		App::new()
			.wrap(middleware::LoggingFactory::new(&threaded_database))
			.app_data(Data::new(threaded_database.clone()))
			.route("/auth", web::post().to(routes::auth::post_auth))
			.route("/meta/logs", web::get().to(routes::meta::get_logs))
			.route("/hitl/test", web::post().to(routes::hitl::post_test))
			.route("/admin/create-user", web::post().to(routes::admin::post_create_user))
			.route("/admin/sql", web::post().to(routes::admin::post_sql))
	}).bind(("127.0.0.1", 7200))?
		.run()
		.await?;

	Ok(())
}
