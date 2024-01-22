/// Converts any arbitrary error type into an Actix Web `ErrorBadRequest`.
/// 
/// This function is intended for use in route functions, where this is a
/// common pattern:
/// 
/// ```
/// database
/// 	.execute("SELECT * FROM NodeMappings")
/// 	.map_err(|error| error::ErrorBadRequest(error.to_string()))?;
/// ```
/// 
/// This simple function replaces this boilerplate mess with the following:
/// 
/// ```
/// database
/// 	.execute("SELECT * FROM NodeMappings")
/// 	.map_err(bad_request)?;
/// ```
/// 
pub fn bad_request(native_error: impl ToString) -> actix_web::Error {
	actix_web::error::ErrorBadRequest(native_error.to_string())
}

/// Converts any arbitrary error type into an Actix Web `ErrorInternalServerError`.
/// 
/// This function is intended for use in route functions, where this is a
/// common pattern:
/// 
/// ```
/// database
/// 	.execute("SELECT * FROM NodeMappings")
/// 	.map_err(|error| error::ErrorInternalServerError(error.to_string()))?;
/// ```
/// 
/// This simple function replaces this boilerplate mess with the following:
/// 
/// ```
/// database
/// 	.execute("SELECT * FROM NodeMappings")
/// 	.map_err(internal)?;
/// ```
/// 
pub fn internal(native_error: impl ToString) -> actix_web::Error {
	actix_web::error::ErrorInternalServerError(native_error.to_string())
}
