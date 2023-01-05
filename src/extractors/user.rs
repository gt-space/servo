use actix_web::{FromRequest, error, http::header, HttpRequest, web::Data};
use crate::Database;
use std::{future::Future, pin::Pin};

pub struct User {
	pub username: String,
	pub is_admin: bool,
}

impl FromRequest for User {
	type Error = actix_web::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

	fn from_request(request: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
		let database = request.app_data::<Data<Database>>()
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