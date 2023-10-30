use actix_web::{dev::{Service, ServiceRequest, ServiceResponse, Transform}, error};
use crate::{Database, extractors::Hostname};
use std::{future::{Future, ready, Ready}, pin::Pin, rc::Rc};

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
			let endpoint = request
				.path()
				.to_owned();

			let origin = request
				.peer_addr()
				.expect("unit tests are currently incompatible with logging middleware")
				.to_string();

			let hostname = request.extract::<Hostname>().await?;
			let locked_database = database.lock().await;

			locked_database
				.execute(
					"INSERT INTO RequestLogs (endpoint, origin, hostname) VALUES (?1, ?2, ?3)",
					rusqlite::params![endpoint, origin, hostname]
				)
				.map_err(|error| error::ErrorInternalServerError(format!("sql error: {}", error.to_string())))?;

			let log_id = locked_database.last_insert_rowid();
			drop(locked_database);

			let response = service.call(request).await?;

			database
				.lock()
				.await
				.execute(
					"UPDATE RequestLogs SET status_code = ?1 WHERE log_id = ?2",
					rusqlite::params![response.status().as_u16(), log_id]
				)
				.map_err(|error| error::ErrorInternalServerError(format!("sql error: {}", error.to_string())))?;

			Ok(response)
		})
	}
}

/// A factory that creates instances of `LoggingMiddleware` to be used in the server
pub struct LoggingFactory {
	database: Database
}

impl LoggingFactory {
	/// Creates a new `LoggerFactory` given a reference to a `Database` (which it clones) 
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
