use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tempsystem", version = version::version)]
#[command(about = "Create and enter a completely temporary system, whenever you want!", long_about = None)]
struct Args {
	#[arg(long, help = "show more verbose output")]
	verbose: bool,

	#[arg(
		short,
		long,
		help = "run a system update before entering; can fix issues with package install fails (recommended with --chaotic-aur or --landware)"
	)]
	update_system: bool,

	#[arg(
		long,
		help = "update the pkgfile database; recommended with --update-system, --chaotic-aur, or --landware, but this can take a while"
	)]
	update_pkgfile: bool,

	#[arg(short, long, help = "mount system root as read only (cannot be used with --extra-packages)")]
	ro_root: bool,

	#[arg(short = 'c', long, help = "mount ~/work as read only")]
	ro_cwd: bool,

	#[arg(short, long, help = "do not mount current directory to ~/work")]
	disable_cwd_mount: bool,

	#[arg(short, long, help = "disable network capabilities for the system (cannot be used with --extra-packages)")]
	no_network: bool,

	#[arg(
		short = 'p',
		long,
		help = "extra packages to install in the system, space deliminated (cannot be used with --no-network or --ro-root)"
	)]
	extra_packages: Option<String>,

	#[arg(short = 'a', long, help = "same as --extra-packages, but fetches the packages from the AUR")]
	extra_aur_packages: Option<String>,

	#[arg(long, help = "give extended privileges to the system")]
	privileged: bool,

	#[arg(long, help = "Add the Chaotic-AUR to the system")]
	chaotic_aur: bool,

	#[arg(long, help = "Add the landware repo to the system")]
	landware: bool,

	#[arg(default_value = "/usr/bin/zsh", help = "command to execute in container, then exit")]
	command: Vec<String>,

	#[cfg(feature = "generators")]
	#[arg(long = "generate-man")]
	generate_man: String,

	#[cfg(feature = "generators")]
	#[arg(value_enum, long = "generate-shell")]
	generate_shell: clap_complete::Shell,
}

mod docker;
use docker::*;
use tokio_util::sync::CancellationToken;

#[macro_export]
macro_rules! print_error {
	($err:expr) => {{
		use colorize::AnsiColor;
		println!("{}", ($err).to_string().red());
	}};
	($msg:expr, $err:expr) => {
		use colorize::AnsiColor;
		println!("{}", format!("{}: {}", ($msg), ($err).to_string()).red());
	};
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
	let args = Args::parse();

	#[cfg(feature = "generators")]
	{
		use clap::CommandFactory;
		let man = clap_mangen::Man::new(Args::command());
		let mut buffer: Vec<u8> = Default::default();
		man.render(&mut buffer).unwrap();
		std::fs::write(args.generate_man, buffer).unwrap();

		use clap_complete::{Generator, Shell, generate};
		clap_complete::aot::generate(args.generate_shell, &mut Args::command(), Args::command().get_name().to_string(), &mut std::io::stdout());

		return 0.into();
	}

	let token = CancellationToken::new();
	let token_clone = token.clone();
	tokio::task::spawn(async move {
		if tokio::signal::ctrl_c().await.is_ok() {
			token_clone.cancel();
		}
	});

	let mut context = Context::default();
	if let Err(e) = context.connect() {
		print_error!(e);
	}

	tokio::select! {
		_ = token.cancelled() => {
			if let Err(e) = context.delete_container().await {
				print_error!("could not delete system after cancel (could be that it did not create the system yet)", e);
			}
		}
		ret = context.perform_all_enter(&args) => {
			match ret {
				Err(e) => {
					print_error!(e);
					print_error!("note: running with --verbose can help in determining error cause");
					if let Err(e) = context.delete_container().await {
						print_error!("could not delete system after error", e);
					}
				}
				Ok(code) => {
					return (code as u8).into();
				}
			}
		}
	}

	return 0.into();
}
