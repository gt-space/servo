use anyhow::anyhow;
use clap::ArgMatches;
use crate::{cache_path, Cache};
use jeflog::{fail, pass, task, warn};
use std::{collections::HashSet, fmt, net::{IpAddr, TcpStream}, path::{Path, PathBuf}, process, time::Duration};
use ssh::LocalShell;

// const SSH_PRIVATE_KEY: &'static str = include_str!("../../keys/id_ed25519");

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Platform {
	AppleSilicon,
	Beaglebone,
	Meerkat,
	RaspberryPi,
}

impl Platform {
	pub fn triple(self) -> &'static str {
		match self {
			Self::AppleSilicon => "aarch64-apple-darwin",
			Self::Beaglebone => "armv7-unknown-linux-musleabihf",
			Self::Meerkat => "x86_64-unknown-linux-gnu",
			Self::RaspberryPi => "aarch64-unknown-linux-musl",
		}
	}

	pub fn default_login(self) -> (&'static str, &'static str) {
		match self {
			Self::AppleSilicon => ("none", "none"),
			Self::Beaglebone => ("debian", "temppwd"),
			Self::Meerkat => ("yjsp", "yjspfullscale"),
			Self::RaspberryPi => ("pi", "p@ssw0rd"),
		}
	}
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Repository {
	Ahrs,
	Flight,
	Gui,
	Sam,
	Servo,
}

impl Repository {
	pub fn remote(self) -> &'static str {
		match self {
			Self::Ahrs => "https://github.com/gt-space/ahrs",
			Self::Flight => "https://github.com/gt-space/flight",
			Self::Gui => "https://github.com/gt-space/gui",
			Self::Sam => "https://github.com/gt-space/sam",
			Self::Servo => "https://github.com/gt-space/servo",
		}
	}

	pub fn fetch(self) -> anyhow::Result<()> {
		let repo_cache = cache_path().join(self.to_string());

		task!("Locating local cache of \x1b[1m{self}\x1b[0m.");

		if repo_cache.is_dir() {
			pass!("Using local cache at \x1b[1m{}\x1b[0m.", repo_cache.to_string_lossy());
			task!("Fetching latest version of branch \x1b[1mmain\x1b[0m.");

			let pull = process::Command::new("git")
				.args(["-C", &repo_cache.to_string_lossy(), "pull"])
				.output();

			if pull.is_ok() {
				pass!("Pulled latest version of branch \x1b[1mmain\x1b[0m.");
			} else {
				// TODO: add print of remote URL
				warn!("Failed to fetch \x1b[1mmain\x1b[0m from remote. Falling back on cached version.");
			}
		} else {
			warn!("Did not find existing local cache.");
			
			let remote = self.remote();
			task!("Cloning remote repository at \x1b[1m{remote}\x1b[0m.");

			let clone = process::Command::new("git")
				.args(["clone", remote, &repo_cache.to_string_lossy()])
				.output();

			if let Err(error) = clone {
				fail!("Failed to clone remote repository at \x1b[1m{remote}\x1b[0m.");
				return Err(error.into());
			}

			pass!("Cloned remote repository at \x1b[1m{remote}\x1b[0m.");
		}

		Ok(())
	}

	pub fn compile_for(self, platform: Platform, path: Option<&Path>) -> anyhow::Result<()> {
		let target = platform.triple();

		let cache_path = Cache::get().path.join(self.to_string());
		let repo_path = path.unwrap_or(&cache_path);

		let manifest_path = repo_path.join("Cargo.toml");
		let config_path = repo_path.join(".cargo/config.toml");

		task!("Building \x1b[1m{self}\x1b[0m for target \x1b[1m{target}\x1b[0m.");

		let mut build = process::Command::new("cargo");

		build
			.args(["build", "--release"])
			.args(["--target", platform.triple()])
			.args(["--manifest-path", &manifest_path.to_string_lossy()]);

		// only set the config.toml path if this repository has one
		if config_path.exists() {
			build.args(["--config", &config_path.to_string_lossy()]);
		}

		let build = build
			.output()
			.unwrap();

		if !build.status.success() {
			fail!("Failed to build \x1b[1m{self}\x1b[0m for target \x1b[1m{target}\x1b[0m.");
			return Err(anyhow!(String::from_utf8(build.stderr).unwrap()));
		}

		pass!("Built \x1b[1m{self}\x1b[0m for target \x1b[1m{target}\x1b[0m.");

		Ok(())
	}
}

impl fmt::Display for Repository {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Ahrs => write!(f, "ahrs"),
			Self::Flight => write!(f, "flight"),
			Self::Gui => write!(f, "gui"),
			Self::Sam => write!(f, "sam"),
			Self::Servo => write!(f, "servo"),
		}
	}
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Target {
	hostname: String,
	assigned_hostname: Option<String>,
	assigned_ip: Option<IpAddr>,
	platform: Platform,
	repository: Repository,
}

impl Target {
	pub fn transfer_binary(&self) -> Result<(), ()> {
		// 1. locate the compiled binary corresponding to this target's associated repo
		task!("Locating compiled binary for \x1b[1m{}\x1b[0m.", self.repository);

		let binary_path = Cache::get().path
			.join(self.repository.to_string())
			.join("target")
			.join(self.platform.triple())
			.join("release")
			.join(self.repository.to_string());

		if !binary_path.exists() {
			fail!("Failed to locate binary for \x1b[1m{}\x1b[0m. Has it been compiled?", self.repository);
			return Err(());
		}

		let (user, password) = self.platform.default_login();

		pass!("Located compiled binary for \x1b[1m{}\x1b[0m.", self.repository);

		// 2. connect to the target machine over SSH
		task!("Connecting to \x1b[1m{}\x1b[0m via SSH.", self.hostname);

		let session = ssh::create_session()
			.username(user)
			.password(password)
			.timeout(Some(Duration::from_secs(5)))
			.connect((self.hostname.as_str(), 22));

		let mut session = match session {
			Ok(session) => session.run_local(),
			Err(error) => {
				fail!("Failed to connect to \x1b[1m{}\x1b[0m: {error}", self.hostname);
				return Err(());
			},
		};

		pass!("Connected to \x1b[1m{}\x1b[0m via SSH.", self.hostname);

		// 3. upload the compiled binary itself onto the target.
		//
		// this file cannot be directly uploaded to /usr/local/bin because
		// scp cannot have root privileges without being root user, so we need to
		// go in manually after transfer and move it with sudo.
		task!("Uploading binary using SCP.");

		let upload_path = PathBuf::from("/tmp");

		let scp = match session.open_scp() {
			Ok(scp) => scp,
			Err(error) => {
				fail!("Failed to initiate SCP upload: {error}");
				return Err(());
			},
		};

		if let Err(error) = scp.upload(&binary_path, &upload_path) {
			fail!("Failed to upload using SCP: {error}");
			return Err(());
		}

		pass!("Uploaded the binary using SCP.");

		// 4. change the metadata of the file to make it executable again.
		//
		// for some reason, the SCP transfer removes the executable flag from the
		// file's metadata. we need to manually use 'chmod' on the target to fix this.
		task!("Modifying metadata of the transferred binary.");

		let exec = match session.open_exec() {
			Ok(exec) => exec,
			Err(error) => {
				fail!("Failed to open execution channel in SSH: {error}");
				return Err(());
			}
		};

		let chmod = format!("chmod +x /tmp/{}", self.repository);

		if let Err(error) = exec.send_command(&chmod) {
			fail!("Failed to modify metadata of the transferred binary: {error}");
			return Err(());
		}

		pass!("Modified metadata of the transferred binary");

		// 5. move the binary over to /usr/local/bin, which is on $PATH.
		//
		// this is necessary so that the binary is globally accessible, not just
		// a file in a random location. this should make it easier to run on boot.
		task!("Moving binary to be globally accessible.");

		let mut shell = match session.open_shell() {
			Ok(shell) => shell,
			Err(error) => {
				fail!("Failed to open shell: {error}");
				return Err(());
			},
		};
		
		let wait_for_ready = |shell: &mut LocalShell<TcpStream>, terminator: char| {
			// loop fetching output until a dollar sign ends a packet,
			// indicating that the shell is ready to receive commands
			loop {
				let mut output = match shell.read() {
					Ok(output) => String::from_utf8_lossy(&output).to_string(),
					Err(error) => {
						fail!("Failed to read shell output on login: {error}");
						return Err(());
					},
				};

				// remove all ANSI control codes such as coloring from the string.
				// these start with byte 0x1B and end with the character 'm'.
				while let Some(ctrl_start) = output.find('\x1b') {
					if let Some(ctrl_len) = output[ctrl_start..].find('m') {
						output.replace_range(ctrl_start..=ctrl_start + ctrl_len, "");
					} else {
						output.remove(ctrl_start);
					}
				}

				// if the output ends with a dollar sign then we know that the shell
				// is prepared to receive commands
				if output.trim_end().ends_with(terminator) {
					break;
				}
			}

			Ok(())
		};

		// 5-1. wait for the shell to be ready after login.
		wait_for_ready(&mut shell, '$')?;

		let mv = format!("sudo mv /tmp/{} /usr/local/bin\n{password}\n", self.repository);
		if let Err(error) = shell.write(mv.as_bytes()) {
			fail!("Failed to move binary to be accessible from $PATH: {error}");
			return Err(());
		}

		// 5-2. wait until the shell is ready for another command before quitting.
		// this indicates that the 'mv' command has been fully executed.
		wait_for_ready(&mut shell, '$')?;

		if let Err(error) = shell.close() {
			warn!("Moved binary, but failed to close shell: {error}");
		} else {
			pass!("Moved binary to be globally accessible.");
		}

		session.close();
		Ok(())
	}
}

/// Compiles and deploys MCFS binaries to respective machines.
pub fn deploy(args: &ArgMatches) {
	let dry = *args.get_one::<bool>("dry").unwrap();
	let offline = *args.get_one::<bool>("offline").unwrap();
	let path = args.get_one::<String>("path").map(|path| PathBuf::from(path));
	let target_names = args.get_one::<String>("targets");

	let mut targets;

	if let Some(target_names) = target_names {
		targets = Vec::new();

		for name in target_names.split(',') {
			let mut hostname = name.to_owned();

			if !hostname.ends_with(".local") && hostname != "localhost" {
				hostname.push_str(".local");
			}

			targets.push(Target {
				hostname,
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::RaspberryPi,
				repository: Repository::Sam,
			});
		}
	} else {
		targets = vec![
			Target {
				hostname: "sam-01.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
			Target {
				hostname: "sam-02.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
			Target {
				hostname: "sam-03.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
			Target {
				hostname: "sam-04.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
			Target {
				hostname: "sam-05.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
			Target {
				hostname: "beaglebone.local".to_owned(),
				assigned_hostname: None,
				assigned_ip: None,
				platform: Platform::Beaglebone,
				repository: Repository::Sam,
			},
		];
	}

	// 1. cache all repositories necessary to deploy to the target list.
	//
	// these are cached in the '~/.servo' directory along with the database.
	if !offline || path.is_some() {
		task!("1. Fetching all required repositories.");
		let mut cached = HashSet::new();
		let mut failed = 0;

		for target in &targets {
			let repo = target.repository;

			if cached.contains(&repo) {
				continue;
			}

			task!("Caching repository \x1b[1m{repo}\x1b[0m.");

			if repo.fetch().is_ok() {
				pass!("Cached repository \x1b[1m{repo}\x1b[0m.");
				cached.insert(repo);
			} else {
				fail!("Failed to cache repository \x1b[1m{repo}\x1b[0m.");
				failed += 1;
			}
		}

		if failed == 0 {
			pass!("1. Fetched all repositories.");
		} else if cached.len() == 0 {
			fail!("1. Failed to fetch any repositories.");
			return;
		} else {
			warn!("1. Fetched {} repositories. Ignoring targets with unfetched repositories.", cached.len());
		}

		targets.retain(|target| cached.contains(&target.repository));
	} else {
		if let Some(path) = &path {
			warn!("1. Using local repository at \x1b[1m{}\x1b[0m.", path.to_string_lossy());
		} else {
			warn!("1. Using local cache because --offline flag was set.");
		}
	}

	println!();

	// 2. compile all repositories for all platforms required by the target list.
	task!("2. Compiling all required repositories.");
	let mut compiled = HashSet::new();

	for target in &targets {
		let repo = target.repository;
		let platform = target.platform;

		if compiled.contains(&(repo, platform)) {
			continue;
		}

		task!("Compiling repository \x1b[1m{repo}\x1b[0m.");

		if let Err(error) = repo.compile_for(platform, path.as_deref().clone()) {
			fail!("Failed to compile repository \x1b[1m{repo}\x1b[0m: {error}");
		} else {
			pass!("Compiled repository \x1b[1m{repo}\x1b[0m.");
		}

		compiled.insert((repo, platform));
	}

	pass!("2. Compiled all required repositories.");
	println!();

	if !dry {
		// 3. transfer the compiled binaries to all targets.
		task!("3. Transferring all compiled binaries of listed targets.");

		for target in targets {
			task!("Uploading binary for \x1b[1m{}\x1b[0m to target \x1b[1m{}\x1b[0m.", target.repository, target.hostname);

			if target.transfer_binary().is_ok() {
				pass!("Uploading binary for \x1b[1m{}\x1b[0m to target \x1b[1m{}\x1b[0m.", target.repository, target.hostname);
			} else {
				fail!("Failed to upload binary for \x1b[1m{}\x1b[0m to target \x1b[1m{}\x1b[0m.", target.repository, target.hostname);
			}
		}

		pass!("3. Transferred all compiled binaries of listed targets.");
	} else {
		warn!("3. Skipping transfer step because the --dry flag was set.")
	}

	println!();
}
