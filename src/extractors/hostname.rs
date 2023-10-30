use actix_web::{FromRequest, web::Data};
use rusqlite::{ToSql, types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef}};
use std::{future::Future, pin::Pin, collections::HashMap, net::IpAddr, sync::Arc};
use tokio::sync::Mutex;

/// Contains an `Option<String>` corresponding to the hostname of a network device.
/// Mainly intended to be extracted from Actix requests.
#[derive(Clone, Debug)]
pub struct Hostname(Option<String>);

/// Contains an `Arc<Mutex<HashMap<IpAddr, Hostname>>>` which stores the mappings
/// between IP addresses and their hostnames.
#[derive(Clone, Debug)]
pub struct HostMap(Arc<Mutex<HashMap<IpAddr, Hostname>>>);

impl HostMap {
	/// Creates an empty `HostMap` with no mappings.
	pub fn new() -> Self {
		HostMap(Arc::new(Mutex::new(HashMap::new())))
	}
}

impl FromRequest for Hostname {
	type Error = actix_web::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

	fn from_request(request: &actix_web::HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
		let host_map = request.app_data::<Data<HostMap>>()
			.expect("hostname map not included in app data")
			.clone();

		let origin = request
			.peer_addr()
			.expect("unit tests are currently incompatible with logging middleware");

		Box::pin(async move {
			let mut host_map = host_map
				.0
				.lock()
				.await;

			if let Some(hostname) = host_map.get(&origin.ip()) {
				return Ok(hostname.clone());
			}

			if let Ok((hostname, _)) = dns_lookup::getnameinfo(&origin, 0) {
				let hostname = Hostname(Some(hostname));
				host_map.insert(origin.ip(), hostname.clone());

				Ok(hostname)
			} else {
				Ok(Hostname(None))
			}
		})
	}
}

impl ToSql for Hostname {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		self.0.to_sql()
	}
}

impl FromSql for Hostname {
	fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
		value
			.as_str_or_null()
			.map(|host| Hostname(
				host.map(|str| str.to_owned())
			))
	}
}
