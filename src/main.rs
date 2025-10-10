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
	extra_packages: Option<String>,

	#[arg(short = 'a', long, help = "same as --extra-packages, but fetches the packages from the AUR")]
	extra_aur_packages: Option<String>,

	#[arg(long, help = "give extended privileges to the system")]
	privileged: bool,

	#[arg(default_value = "/usr/bin/zsh", help = "command to execute in container, then exit")]
	command: Vec<String>,
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
				print_error!("could not delete system after error", e);
			}
		}
		ret = context.perform_all_enter(&args) => {
			match ret {
				Err(e) => {
					print_error!(e);
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
