#![warn(missing_docs)]

//! Servo is the library/binary hybrid written for the Yellow Jacket Space Program's control server.

/// Actix-web extractors available to the various server routes.
pub mod extractors;

/// Components relevant to forwarding UDP datagrams to multiple targets.
pub mod forwarding;

/// Pre- and post-route middleware which does extra work with requests (ex. logging).
pub mod middleware;

/// All functions defining API routes.
pub mod routes;

use std::sync::Arc;

/// A convenience type representing a `rusqlite::Connection` that may be passed to multiple async
/// contexts at once.
pub type Database = Arc<tokio::sync::Mutex<rusqlite::Connection>>;
