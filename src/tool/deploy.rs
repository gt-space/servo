use crate::tool::log::{task, subtask, pass, fail};
// use ssh2::Session as SshSession;

use std::{
	path::{Path, PathBuf},
	env,
	fs,
	process,
	net::{IpAddr, TcpStream},
};

const SSH_PRIVATE_KEY: &'static str = include_str!("../../keys/id_ed25519");

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Target {
	Beaglebone,
	Meerkat,
}

impl Target {
	pub fn triple(&self) -> &'static str {
		match self {
			Self::Beaglebone => "armv7-unknown-linux-gnueabihf",
			Self::Meerkat => "x86_64-unknown-linux-gnu",
		}
	}
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum HostType {
	ControlServer,
	FlightComputer,
	GroundComputer,
	SamBoard,
	Gui,
}

impl HostType {
	pub fn target(&self) -> Target {
		match self {
			Self::ControlServer => Target::Meerkat,
			Self::FlightComputer => Target::Beaglebone,
			Self::GroundComputer => Target::Beaglebone,
			Self::SamBoard => Target::Beaglebone,
			Self::Gui => Target::Meerkat,
		}
	}

	pub fn repository(&self) -> &'static str {
		match self {
			Self::ControlServer => "servo",
			Self::FlightComputer | Self::GroundComputer => "fs-flight-computer",
			Self::SamBoard => "fs-sam-software",
			Self::Gui => "fs-gui",
		}
	}
}

struct Host {
	name: &'static str,
	ip_address: IpAddr,
	host_type: HostType,
	user: &'static str,
	password: &'static str,
}

impl Host {
	pub fn find(name: &'static str, host_type: HostType, user: &'static str, password: &'static str) -> Option<Self> {
		subtask!("Locating \x1b[1m{name}\x1b[0m.");

		let ip_address = dns_lookup::lookup_host(name)
			// .or_else(|_| dns_lookup::lookup_host(&format!("{name}.local")))
			.ok()
			.and_then(|addresses|
				addresses
					.iter()
					.find(|ip| ip.is_ipv4())
					.cloned()
			);

		if let Some(ip_address) = ip_address {
			pass!("Found \x1b[1m{name}\x1b[0m at \x1b[1m{ip_address}\x1b[0m.");
			Some(Host { name, ip_address, host_type, user, password })
		} else {
			fail!("Failed to locate \x1b[1m{name}\x1b[0m.");
			None
		}
	}
}

// struct DeploymentClient;

// impl thrussh::client::Handler for DeploymentClient {
// 	type Error = anyhow::Error;
// 	type FutureBool = Ready<Result<(Self, bool), Self::Error>>;
// 	type FutureUnit = Ready<Result<(Self, thrussh::client::Session), Self::Error>>;

// 	fn finished_bool(self, b: bool) -> Self::FutureBool {
// 		ready(Ok((self, b)))
// 	}

// 	fn finished(self, session: thrussh::client::Session) -> Self::FutureUnit {
// 		ready(Ok((self, session)))
// 	}

// 	fn check_server_key(self, _public_key: &key::PublicKey) -> Self::FutureBool {
// 		// automatically accept any server key as valid
// 		self.finished_bool(true)
// 	}
// }

fn fetch_repository(repo: &str, cache_path: PathBuf, cache_display_path: &str) -> anyhow::Result<()> {
	subtask!("Locating local cache of \x1b[1m{repo}\x1b[0m.");

	let repo_cache = cache_path.join(repo);

	let cache_string = cache_path
		.to_string_lossy()
		.into_owned();

	if repo_cache.exists() {
		pass!("Using cache found at \x1b[1m{}\x1b[0m.", cache_display_path);
		subtask!("Pulling latest version of branch \x1b[1mmain\x1b[0m from GitHub.");

		process::Command::new("git")
			.args(["pull", "-C", &repo_cache.to_string_lossy()])
			.output()?;

		pass!("Pulled latest version of branch \x1b[1mmain\x1b[0m from GitHub.");
	} else {
		fail!("Failed to locate cache.");

		let clone_address = format!("git@github-research.gatech.edu:YJSP/{repo}");

		subtask!("Cloning GitHub repository at \x1b[1m{clone_address}\x1b[0m.");

		let clone_succeeded = process::Command::new("git")
			.args(["clone", &clone_address, &cache_string])
			.output()?
			.status
			.success();

		if clone_succeeded {
			pass!("Cloned GitHub repository at \x1b[1m{clone_address}\x1b[0m.");
		} else {
			fail!("Failed to clone GitHub repository at \x1b[1m{clone_address}\x1b[0m.");
		}
	}

	Ok(())
}

fn compile_for_target(repo_path: &Path, target: Target) -> anyhow::Result<()> {
	let manifest_path = repo_path
		.join("Cargo.toml")
		.to_string_lossy()
		.into_owned();

	subtask!("Compiling for target \x1b[1m{}\x1b[0m.", target.triple());

	let compilation = process::Command::new("cross")
		.args(["build", "--release"])
		.args(["--target", target.triple()])
		.args(["--manifest-path", &manifest_path])
		.output()?;

	if !compilation.status.success() {
		fail!("Compilation failed with code \x1b[1m{}\x1b[0m.", compilation.status.code().unwrap());

		println!("\n{}", String::from_utf8(compilation.stderr).unwrap());

		return Ok(());
	}

	pass!("Compiled for target \x1b[1m{}\x1b[0m.", target.triple());

	Ok(())
}

async fn transfer_to_target(host: &Host) {
	let user = host.user;
	let address = host.ip_address;

	subtask!("Establishing SSH connection to \x1b[1m{user}@{address}\x1b[0m.");

	let stream = TcpStream::connect((address, 22)).unwrap();
	// let mut session = SshSession::new().unwrap();
	// session.set_tcp_stream(stream);
	// session.handshake();

	// session.userauth_pubkey_memory("yjsp", None, SSH_PRIVATE_KEY, None);

	// let sftp = session.sftp().unwrap();

	// let session = thrussh::client::connect(
	// 	Arc::new(thrussh::client::Config::default()),
	// 	(address, 22),
	// 	DeploymentClient,
	// ).await;

	// if let Ok(mut session) = session {
	// 	let auth = session.authenticate_password(user, host.password).await;

	// 	// authentication failed
	// 	if !auth.unwrap_or(false) {
	// 		fail!("Failed to authenticate to \x1b[1m{user}@{address}\x1b[0m.");
	// 		return;
	// 	}

	// 	pass!("Authenticated to \x1b[1m{user}@{address}\x1b[0m.");

	// 	let channel = session.channel_open_session().await.unwrap();
	// 	// channel.exec
	// 	channel.exec(false, "tee");
	// } else {
	// 	fail!("Failed to establish SSH connection to \x1b[1m{}@{address}\x1b[0m.", user);
	// 	return;
	// }
}

/// Compiles and deploys MCFS binaries to respective machines.
/// 
pub async fn deploy() -> anyhow::Result<()> {
	task!("Locating deployable targets on the network.");

	let hosts = [
		Host::find("server-01", HostType::ControlServer, "yjsp", "yjspfullscale"),
		Host::find("server-02", HostType::ControlServer, "yjsp", "yjspfullscale"),
		Host::find("flight-01", HostType::FlightComputer, "yjsp", "yjspfullscale"),
		Host::find("flight-02", HostType::FlightComputer, "yjsp", "yjspfullscale"),
		Host::find("ground-01", HostType::GroundComputer, "yjsp", "yjspfullscale"),
		Host::find("ground-02", HostType::GroundComputer, "yjsp", "yjspfullscale"),
		Host::find("sam-01", HostType::SamBoard, "debian", "temppwd"),
		Host::find("sam-02", HostType::SamBoard, "debian", "temppwd"),
		Host::find("sam-03", HostType::SamBoard, "debian", "temppwd"),
		Host::find("sam-04", HostType::SamBoard, "debian", "temppwd"),
		Host::find("gui-01", HostType::Gui, "yjsp", "yjspfullscale"),
		Host::find("gui-02", HostType::Gui, "yjsp", "yjspfullscale"),
		Host::find("gui-03", HostType::Gui, "yjsp", "yjspfullscale"),
		Host::find("gui-04", HostType::Gui, "yjsp", "yjspfullscale"),
	].into_iter().filter_map(|host| host).collect::<Vec<Host>>();

	if hosts.len() > 0 {
		pass!("Located \x1b[1m{}\x1b[0m deployable host{}", hosts.len(), if hosts.len() > 1 { "s" } else { "" });
	} else {
		fail!("Did not locate any deployable targets on the network.");
		return Ok(());
	}

	println!();

	let cache_path;
	let cache_display_path;

	if cfg!(target_os = "macos") {
		cache_path = PathBuf::from(env::var("HOME")?).join("Library/Caches/servo");
		cache_display_path = "~/Library/Caches/servo";
	} else if cfg!(target_os = "windows") {
		cache_path = PathBuf::from(env::var("LOCALAPPDATA")?).join("servo");
		cache_display_path = "%LOCALAPPDATA%\\servo";
	} else {
		cache_path = PathBuf::from("/var/cache/servo");
		cache_display_path = "/var/cache/servo";
	}

	fs::create_dir_all(&cache_path)?;

	for host in hosts {
		let repo = host.host_type.repository();

		task!("Deploying latest version of \x1b[1m{repo}\x1b[0m for \x1b[1m{}\x1b[0m.", host.name);

		fetch_repository(repo, cache_path.clone(), cache_display_path)?;
		compile_for_target(&cache_path.join(repo), host.host_type.target())?;
		transfer_to_target(&host).await;

		if false {
			fail!("Failed to deploy latest version of \x1b[1m{repo}\x1b[0m for \x1b[1m{}\x1b[0m.", host.name);
		}

		pass!("Deployed \x1b[1m{repo}\x1b[0m for \x1b[1m{}\x1b[0m.", host.name);
	}

	Ok(())
}
