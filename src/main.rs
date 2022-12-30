mod hitl;
mod protocol;
mod routes;

use actix_web::{
	App,
	dev::{Service, ServiceResponse, ServiceRequest, Transform},
	error,
	FromRequest,
	http::header,
	HttpServer,
	web::{self, Data},
};

use std::{
	env,
	fs,
	future::{Future, ready, Ready},
	path::Path,
	pin::Pin,
	rc::Rc,
	sync::Arc,
	time::SystemTime,
};

use argon2::password_hash::SaltString;
use rand::rngs::OsRng;
use rusqlite::{Connection as SqlConnection};
use tokio::sync::Mutex;

pub type ThreadedDatabase = Arc<Mutex<rusqlite::Connection>>;

pub struct User {
	username: String,
	is_admin: bool,
}

impl FromRequest for User {
	type Error = actix_web::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

	fn from_request(request: &actix_web::HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
		let database = request.app_data::<Data<ThreadedDatabase>>()
			.expect("database not included in app data")
			.clone();

		let session_id = request
			.headers()
			.get(header::AUTHORIZATION)
			.and_then(|header| header.to_str().ok())
			.filter(|&header| header.starts_with("Bearer "))
			.map(|bearer_id| bearer_id[7..].to_owned())
			.ok_or(error::ErrorUnauthorized("authorization is required for this request"));

		Box::pin(async move {
			let database = database
				.lock()
				.await;

			session_id.and_then(|session_id| {
				database
					.query_row("
						SELECT S.username, U.is_admin
						FROM Sessions AS S
						INNER JOIN Users AS U
						ON U.username = S.username
						WHERE S.session_id = ?1",
						rusqlite::params![session_id],
						|row| Ok(User { username: row.get(0)?, is_admin: row.get(1)? }))
					.map_err(|error| {
						match error {
							rusqlite::Error::QueryReturnedNoRows => error::ErrorUnauthorized("no session found"),
							_ => error::ErrorInternalServerError("sql error"),
						}
					})
			})
		})
	}
}

struct LoggingMiddleware<S> {
	database: ThreadedDatabase,
	service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for LoggingMiddleware<S>
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	S::Future: 'static,
	B: 'static,
{
	type Response = ServiceResponse<B>;
	type Error = actix_web::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

	actix_web::dev::forward_ready!(service);

	fn call(&self, mut request: ServiceRequest) -> Self::Future {
		let database = self.database.clone();
		let service = self.service.clone();

		Box::pin(async move {
			let log_id = uuid::Uuid::new_v4()
				.to_string();

			let endpoint = request
				.path()
				.to_owned();

			let origin = request
				.peer_addr()
				.expect("unit tests are currently incompatible with logging middleware")
				.to_string();

			let username = request.extract::<User>()
				.await
				.map(|user| user.username)
				.ok();

			let timestamp = SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.expect("time is running backwards")
				.as_secs();

			database
				.lock()
				.await
				.execute(
					"INSERT INTO RequestLogs VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
					rusqlite::params![log_id, endpoint, origin, username, timestamp]
				)
				.map_err(|_| error::ErrorInternalServerError("sql error"))?;

			let response = service.call(request).await?;

			database
				.lock()
				.await
				.execute(
					"UPDATE RequestLogs SET status_code = ?1 WHERE log_id = ?2",
					rusqlite::params![response.status().as_u16(), log_id]
				)
				.map_err(|_| error::ErrorInternalServerError("sql error"))?;

			Ok(response)
		})
	}
}

struct LoggingMiddlewareFactory {
	database: ThreadedDatabase
}

impl LoggingMiddlewareFactory {
	pub fn new(database: &ThreadedDatabase) -> Self {
		LoggingMiddlewareFactory {
			database: database.clone()
		}
	}
}

impl<S, B> Transform<S, ServiceRequest> for LoggingMiddlewareFactory
where
	S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
	S::Future: 'static,
	B: 'static,
{
	type Response = ServiceResponse<B>;
	type Error = actix_web::Error;
	type InitError = ();
	type Transform = LoggingMiddleware<S>;
	type Future = Ready<Result<Self::Transform, Self::InitError>>;

	fn new_transform(&self, service: S) -> Self::Future {
		ready(Ok(LoggingMiddleware {
			database: self.database.clone(),
			service: Rc::new(service),
		}))
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let hitl_dir = Path::new(&env::var("HOME")?)
		.join(".hitl");

	if !hitl_dir.is_dir() {
		fs::create_dir(&hitl_dir).unwrap();
	}

	let database = SqlConnection::open(hitl_dir.join("database.sqlite"))?;
	let root_salt = SaltString::generate(&mut OsRng).to_string();

	database.execute_batch(include_str!("./database_schema.sql"))?;
	database.execute("INSERT OR IGNORE INTO Users VALUES ('root', NULL, ?1, 1)", rusqlite::params![root_salt])?;

	let threaded_database = Arc::new(Mutex::new(database));

	HttpServer::new(move || {
		App::new()
			.wrap(LoggingMiddlewareFactory::new(&threaded_database))
			.app_data(web::Data::new(threaded_database.clone()))
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
