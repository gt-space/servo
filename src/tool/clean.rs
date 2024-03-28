use jeflog::{fail, pass, task};
use std::{env, fs, path::Path};

use crate::{cache_path, Cache};

/// Simple tool function used to clean the servo directory and database.
pub fn clean(servo_dir: &Path) {
	let cache = Cache::get();


	task!("Cleaning {cache}.");
	
	if let Err(error) = cache.clean() {
		fail!("Failed to clean cache: {error}");
		return;
	}

	pass!("Cleaned {cache}.");
}
