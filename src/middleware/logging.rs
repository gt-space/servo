use actix_web::{dev::{Service, ServiceRequest, ServiceResponse, Transform}, error};
use crate::{Database, extractors::User};
use std::{future::{Future, ready, Ready}, pin::Pin, rc::Rc, time::{self, SystemTime}};

pub struct LoggingMiddleware<S> {
	database: Database,
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
				.duration_since(time::UNIX_EPOCH)
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

pub struct LoggingFactory {
	database: Database
}

impl LoggingFactory {
	pub fn new(database: &Database) -> Self {
		LoggingFactory {
			database: database.clone()
		}
	}
}

impl<S, B> Transform<S, ServiceRequest> for LoggingFactory
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
