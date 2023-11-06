use servo::tool;
use std::{env, fs, path::Path, process};
use clap::{Command, Arg};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
			Command::new("deploy")
				.about("Deploys YJSP software to all available computers on the network.")
		)
		.subcommand(
			Command::new("emulate")
				.about("Emulates a particular subsystem of the YJSP software stack.")
		)
		.subcommand(
			Command::new("export")
				.about("Exports vehicle state data from a specified timestamp to a specified timestamp.")
				.arg(
					Arg::new("output_path")
						.required(true)
						.short('o')
				)
				.arg(
					Arg::new("from")
						.required(false)
						.long("from")
						.value_parser(clap::value_parser!(f64))
				)
				.arg(
					Arg::new("to")
						.required(false)
						.long("to")
						.value_parser(clap::value_parser!(f64))
				)
		)
		.subcommand(
			Command::new("run")
				.about("Sends a Python sequence to be run on the flight computer.")
				.arg(
					Arg::new("path")
						.required(true)
				)
		)
		.subcommand(
			Command::new("serve")
				.about("Starts the servo server.")
		)
		.subcommand(
			Command::new("sql")
				.about("Executes a SQL statement on the control server database and displays the result.")
				.arg(
					Arg::new("raw_sql")
						.required(true)
				)
		)
		.get_matches();
	
	match matches.subcommand() {
		Some(("deploy", _)) => tool::deploy().await?,
		Some(("emulate", _)) => tool::emulate().await?,
		Some(("export", args)) => {
			tool::export(
				args.get_one::<f64>("from").copied(),
				args.get_one::<f64>("to").copied(),
				args.get_one::<String>("output_path").unwrap(),
			).await?
		},
		Some(("run", args)) => tool::run(args.get_one::<String>("path").unwrap()).await?,
		Some(("serve", _)) => tool::serve(&servo_dir).await?,
		Some(("sql", args)) => tool::sql(args.get_one::<String>("raw_sql").unwrap()).await?,
		_ => {
			eprintln!("Error: Invalid command. Please check the command you entered.");
			process::exit(1);
		}
	};

	Ok(())
}
