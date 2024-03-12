use axum::{http::StatusCode, response::IntoResponse};

/// Any error that the server can throw in a route function.
#[derive(Debug)]
pub enum ServerError {
	/// Error originating from a SQL query.
	Sql(rusqlite::Error),
	
	/// Error that may be converted directly into a `Response`.
	Raw(String, StatusCode)
}

impl Into<ServerError> for rusqlite::Error {
	fn into(self) -> ServerError {
		ServerError::Sql(self)
	}
}

impl IntoResponse for ServerError {
	fn into_response(self) -> axum::response::Response {
		match self {
			Self::Sql(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
			Self::Raw(message, status) => (status, message),
		}.into_response()
	}
}

/// A `Result` type containing a `ServerError` as its `Err` variant.
pub type ServerResult<T> = Result<T, ServerError>;

pub fn bad_request(message: impl ToString) -> ServerError {
	ServerError::Raw(message.to_string(), StatusCode::BAD_REQUEST)
}

pub fn not_found(message: impl ToString) -> ServerError {
	ServerError::Raw(message.to_string(), StatusCode::NOT_FOUND)
}

/// Converts any arbitrary error type into a standardized `ServerError`.
pub fn internal(message: impl ToString) -> ServerError {
	ServerError::Raw(message.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
}
