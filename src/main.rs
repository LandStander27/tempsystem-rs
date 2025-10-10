use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tempsystem", version = version::version)]
#[command(about = "Create and enter a completely temporary system, whenever you want!", long_about = None)]
struct Args {
	#[arg(short, long, help = "run a system update before entering")]
	update_system: bool,

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
	extra_packages: bool,

	#[arg(short = 'a', long, help = "same as --extra-packages, but fetches the packages from the AUR")]
	extra_aur_packages: bool,

	#[arg(long, help = "give extended privileges to the system")]
	privileged: bool,

	#[arg(default_value = "/usr/bin/zsh", help = "command to execute in container, then exit")]
	command: Vec<String>,
}

mod docker;
use docker::*;

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
async fn main() {
	let args = Args::parse();

	let mut context = Context::default();
	if let Err(e) = context.connect() {
		print_error!(e);
	}

	if let Err(e) = context.perform_all_enter(&args).await {
		print_error!(e);
	}
}
