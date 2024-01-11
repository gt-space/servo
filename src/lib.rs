#![warn(missing_docs)]

//! Servo is the library/binary hybrid written for the Yellow Jacket Space Program's control server.

/// Components related to the database connection and maintenance
pub mod database;
pub use database::Database;

/// Actix-web extractors available to the various server routes.
pub mod extractors;

/// Components related to communication with the flight computer
pub mod flight;

/// Components related to forwarding sessions and channel preparation/management.
pub mod forwarding;

/// Components related to interacting with the terminal and developer display
pub mod interface;

/// Pre- and post-route middleware which does extra work with requests (ex. logging).
pub mod middleware;

/// All functions defining API routes.
pub mod routes;

/// Everything related to the Servo command line tool.
pub mod tool;
