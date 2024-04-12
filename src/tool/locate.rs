use clap::ArgMatches;
use jeflog::{fail, pass, warn};
use std::net::ToSocketAddrs;

/// Tool function which locates all known hostnames on the network.
pub fn locate(args: &ArgMatches) {
	let mut prefixes = vec![("server", 3), ("flight", 2), ("ground", 2), ("gui", 6), ("sam", 6)];

	if let Some(subsystem) = args.get_one::<String>("subsystem") {
		let chosen = prefixes
			.iter()
			.position(|(prefix, _)| subsystem == prefix);

		if let Some(chosen) = chosen {
			prefixes = vec![prefixes[chosen]];
		} else {
			fail!("Invalid subsystem / device hostname prefix '{subsystem}'.");
			return;
		}
	}

	for (prefix, count) in prefixes {
		for i in 1..=count {
			let hostname = format!("{prefix}-{i:0>2}.local");

			if let Ok(mut socket_addresses) = (hostname.as_str(), 1).to_socket_addrs() {
				let ip = socket_addresses
					.find(|address| address.is_ipv4())
					.map(|ipv4| ipv4.ip());

				if let Some(ip) = ip {
					pass!("Located \x1b[1m{hostname}\x1b[0m at \x1b[1m{ip}\x1b[0m.");
				} else {
					warn!("Located \x1b[1m{hostname}\x1b[0m at an IPv6 address.");
				}
			} else {
				fail!("Failed to locate \x1b[1m{hostname}\x1b[0m.");
			}
		}
	}
}
