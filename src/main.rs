use servo::tool;
use std::{env, fs, path::Path, process};
use clap::Command;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::init();

	#[cfg(target_family = "windows")]
	let home_path = &env::var("USERPROFILE")?;

	#[cfg(target_family = "unix")]
	let home_path = &env::var("HOME")?;

	let servo_dir = Path::new(home_path).join(".servo");

	if !servo_dir.is_dir() {
		fs::create_dir(&servo_dir).unwrap();
	}

	let matches = Command::new("servo")
		.about("Servo command line tool")
		.subcommand_required(true)
		.subcommand(
			Command::new("serve")
				.about("Starts the servo server.")
		)
		.subcommand(
			Command::new("deploy")
				.about("Deploys YJSP software to all available computers on the network.")
		)
		.get_matches();
	
	match matches.subcommand() {
		Some(("serve", _)) => tool::serve(&servo_dir).await?,
		Some(("deploy", _)) => tool::deploy().await?,
		_ => {
			eprintln!("Error: Invalid command. Please check the command you entered.");
			process::exit(1);
		}
	};

	Ok(())
}
