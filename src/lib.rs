#![warn(missing_docs)]

//! Servo is the library/binary hybrid written for the Yellow Jacket Space Program's control server.

/// Components related to interacting with the terminal and developer display
pub mod interface;

/// Components related to the server, including route functions, forwarding, flight communication, and the interface.
pub mod server;

/// Everything related to the Servo command line tool.
pub mod tool;

use jeflog::warn;
use std::{env, fmt, fs, io, path::{Path, PathBuf}, sync::OnceLock};

static CACHE: OnceLock<Cache> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct Cache {
	/// The path of the cache directory.
	pub path: PathBuf
}

impl Cache {
	/// Gets an instance of the local cache by using or setting the singleton.
	pub fn get() -> Self {
		if let Some(cache) = CACHE.get() {
			return cache.clone();
		}

		let home_path;

		if cfg!(target_family = "windows") {
			home_path = env::var("USERPROFILE")
				.expect("%USERPROFILE% environment variable not set.");
		} else {
			home_path = env::var("HOME")
				.expect("$HOME environment variable not set.");
		}

		let cache = Cache {
			path: Path::new(&home_path).join(".servo")
		};

		if !cache.path.is_dir() {
			fs::create_dir(&cache.path).unwrap();
		}

		if CACHE.set(cache.clone()).is_err() {
			warn!("CACHE_PATH static set multiple times. This should not be possible.");
		}

		cache
	}

	/// Cleans the cache directory by removing it.
	pub fn clean(&self) -> io::Result<()> {
		fs::remove_dir_all(&self.path)
	}
}

impl fmt::Display for Cache {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "~/.servo")
	}
}

/// Returns the path to the Servo cache directory on the current machine.
/// 
/// If the cache directory does not already exist, this function creates it.
pub fn cache_path() -> PathBuf {
	static CACHE_PATH: OnceLock<PathBuf> = OnceLock::new();

	if let Some(path) = CACHE_PATH.get() {
			return path.clone();
	}

	let home_path;

	if cfg!(target_family = "windows") {
		home_path = env::var("USERPROFILE")
			.expect("%USERPROFILE% environment variable not set.");
	} else {
		home_path = env::var("HOME")
			.expect("$HOME environment variable not set.");
	}

	let cache_dir = Path::new(&home_path).join(".servo");

	if !cache_dir.is_dir() {
		fs::create_dir(&cache_dir).unwrap();
	}

	if CACHE_PATH.set(cache_dir.clone()).is_err() {
		warn!("CACHE_PATH static set multiple times. This should not be possible.");
	}

	cache_dir
}
