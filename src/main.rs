use anyhow::{anyhow, bail};
use clap::{Arg, ArgAction, Command, value_parser};
use log::{debug, error, info, warn};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, ErrorKind, Write, stdin};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

type Result = anyhow::Result<()>;

fn main() -> Result {
	// Setup logger.
	// UNWRAP: No other logger has or will ever be set
	fern::Dispatch::new()
		.format(|out, args, record| out.finish(format_args!("[{}] {} {args}", record.level(), record.target())))
		.chain(io::stderr())
		.apply()
		.unwrap();

	// Builder for command line parsing
	let command = Command::new("tcp-tool")
		.version(env!("CARGO_PKG_VERSION"))
		.arg(
			Arg::new("tx")
				.short('s')
				.long("send")
				.help("File to send")
				.value_parser(value_parser!(PathBuf)),
		)
		.arg(
			Arg::new("rx")
				.short('r')
				.long("receive")
				.help("File to write from connection")
				.value_parser(value_parser!(PathBuf)),
		)
		.arg(
			Arg::new("dump-source")
				.long("dump-source")
				.help(concat!("Writes the source code of ", env!("CARGO_PKG_NAME"), " to the specified path"))
				.value_parser(value_parser!(PathBuf)),
		)
		.arg(
			Arg::new("address")
				.required(true)
				.short('a')
				.long("address")
				.value_parser(value_parser!(SocketAddr)),
		)
		.arg(Arg::new("yes").short('y').action(ArgAction::SetTrue))
		.arg(
			Arg::new("trigger-port")
				.short('t')
				.long("trigger-port")
				.help("Sends an outbound packet to ensure the gateway's port triggering rules enable port forwarding")
				.value_parser(value_parser!(u16)),
		);

	// Parse command line arguments.
	let args = command.get_matches();
	let yes = args.get_flag("yes");
	let mut prerun_completed = false;

	// UNWRAP: Argument enforced by clap.
	let address = args.get_one::<SocketAddr>("address").unwrap();

	// Pre-run tasks.
	thread::scope(|top| -> Result {
		// Dump source files.
		if let Some(path) = args.get_one::<PathBuf>("dump-source") {
			prerun_completed = true;
			let mut options = OpenOptions::new();
			let src = path.join("src");

			options.create(true).write(true).truncate(true);
			fs::create_dir_all(&src).ok();

			for (path, bytes) in [
				(path.join("Cargo.lock"), include_bytes!("../Cargo.lock").as_slice()),
				(path.join("Cargo.toml"), include_bytes!("../Cargo.toml")),
				(src.join("main.rs"), include_bytes!("main.rs")),
				(path.join("README.md"), include_bytes!("../README.md")),
			] {
				let mut file = options.open(path)?;

				top.spawn(move || file.write_all(bytes));
			}
		}

		// Trigger port with garbage out-bound packet.
		if let Some(&port) = args.get_one::<u16>("trigger-port") {
			prerun_completed = true;

			top.spawn(move || -> Result {
				use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};

				// Send a TCP packet to WAN via the port, to tell the router to open the port.
				// This is for port triggering, a "safer" method of forwarding ports.
				let wan_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(8, 8, 8, 8), port));

				info!("Port triggering requested {wan_addr}");

				//we don't really care about the return value
				thread::spawn(move || {
					let connection_result = TcpStream::connect_timeout(&wan_addr, Duration::from_millis(100));

					match connection_result {
						Ok(..) => info!("Port trigger request ponged."),
						Err(error) if error.kind() == ErrorKind::TimedOut => info!("Port triggering thread shutdown."),
						Err(error) => debug!("Port triggering: {error:?}\n\t(Port triggering only needs an outbound packet to function :+1:)"),
					}
				});

				Ok(())
			});
		}

		Ok(())
	})?;

	if prerun_completed {
		info!("Pre-run tasks complete.");
	}

	// The actual networking.
	match (args.get_one::<PathBuf>("rx"), args.get_one::<PathBuf>("tx")) {
		// Receive file
		(Some(dst_path), None) => {
			let dst_file = OpenOptions::new().write(true).truncate(true).create(true).open(&dst_path)?;
			let mut writer = BufWriter::new(dst_file);
			let listener = TcpListener::bind(&address)?;
			let mut server = None;

			info!("Listener bound to {}", listener.local_addr()?);

			'selection: for incoming in listener.incoming() {
				let Ok(candidate) = incoming else {
					warn!("Failed incoming connection: {}", incoming.unwrap_err());
					continue;
				};

				let Ok(peer_addr) = candidate.peer_addr() else {
					error!("Rejecting candidate without visible peer address.\n{candidate:?}");
					continue;
				};

				if yes {
					info!("Auto-accepting connection {peer_addr}");
				} else {
					let mut stdin = stdin().lock();

					loop {
						let mut buffer = String::new();

						info!("Incoming connection from {peer_addr}, accept?\n( Y / N )");
						stdin.read_line(&mut buffer)?;

						let buffer = buffer.trim().to_lowercase();

						match &*buffer {
							"y" | "yes" => break,
							"n" | "no" => continue 'selection,
							_ => error!("Only accepts Y/N"),
						}
					}
				}

				server = Some(candidate);
				break 'selection;
			}

			let mut server = server.ok_or(anyhow!("Failed to acquire server connection"))?;

			info!("Receiving..");
			io::copy(&mut server, &mut writer)?;
			info!("Finished receiving.");

			Ok(())
		}

		// Send file
		(None, Some(src_path)) => {
			let src_file = OpenOptions::new().read(true).open(&src_path)?;
			let mut reader = BufReader::new(src_file);

			let stream = 'retry: loop {
				let stream_result = TcpStream::connect(&address);

				match stream_result {
					Ok(stream) => break 'retry stream,
					Err(error) if error.kind() == ErrorKind::TimedOut => info!("Waiting for a client to connect..."),
					Err(error) => return Err(error.into()),
				};
			};

			let peer = stream.peer_addr()?;
			let mut writer = BufWriter::new(stream);

			info!("Connected to {peer}, sending...");
			io::copy(&mut reader, &mut writer)?;
			info!("Sent.");

			Ok(())
		}

		(Some(..), Some(..)) => bail!("Cannot use `--send` and `--receive` simultaneously"),
		(None, None) => bail!("You must specify a file with `--send` or `--receive`"),
	}
}
