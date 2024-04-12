use crate::Cache;
use clap::ArgMatches;
use jeflog::{fail, pass, task};

/// Simple tool function used to clean the servo directory and database.
pub fn clean(_args: &ArgMatches) {
	let cache = Cache::get();
	task!("Cleaning {cache}.");
	
	if let Err(error) = cache.clean() {
		fail!("Failed to clean cache: {error}");
		return;
	}

	pass!("Cleaned {cache}.");
}
