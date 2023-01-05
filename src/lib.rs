pub mod control;
pub mod extractors;
pub mod forwarding;
pub mod middleware;
pub mod routes;

use std::sync::Arc;
use tokio::sync::Mutex;

pub type Database = Arc<Mutex<rusqlite::Connection>>;
