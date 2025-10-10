use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tempsystem", version = version::version)]
#[command(about = "Create and enter a completely temporary system, whenever you want!", long_about = None)]
struct Args {}

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
	let mut context = Context::default();
	if let Err(e) = context.connect() {
		print_error!(e);
	}

	if let Err(e) = context.perform_all_enter().await {
		print_error!(e);
	}
}
