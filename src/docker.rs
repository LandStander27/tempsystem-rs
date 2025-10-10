use std::{
	collections::HashMap,
	io::{Read, Write},
	time::Duration,
};

use bollard::Docker;
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use termion::{async_stdin, raw::IntoRawMode, terminal_size};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::{Args, print_error};

#[derive(Error, Debug)]
pub enum Error {
	#[error("could not connect to docker: {0}")]
	Connection(bollard::errors::Error),

	#[error("inner error: docker not connected")]
	NotConnected,

	#[error("could not create image: {0}")]
	ImageCreate(bollard::errors::Error),

	#[error("could not create container: {0}")]
	ContainerCreate(bollard::errors::Error),

	#[error("could not start container: {0}")]
	ContainerStart(bollard::errors::Error),

	#[error("could not create exec: {0}")]
	ExecCreate(bollard::errors::Error),

	#[error("could not start exec: {0}")]
	ExecStart(bollard::errors::Error),

	#[error("exec was expected to be attached")]
	ExpectedAttached,

	#[error("exec was expected to be detached")]
	ExpectedDetached,

	#[error("could not recv terminal size: {0}")]
	TerminalSize(std::io::Error),

	#[error("could not resize exec: {0}")]
	ExecResize(bollard::errors::Error),

	#[error("could not inspect exec: {0}")]
	ExecInspect(bollard::errors::Error),

	// #[error("could not get exit code")]
	// ExecExitcode,
	#[error("could not set raw mode: {0}")]
	Rawmode(std::io::Error),

	#[error("could not write to stdout: {0}")]
	StdoutWrite(std::io::Error),

	#[error("could flush stdout: {0}")]
	StdoutFlush(std::io::Error),

	#[error("could not delete container: {0}")]
	ContainerDelete(bollard::errors::Error),

	#[error("could not get cwd: {0}")]
	GetCWD(std::io::Error),

	#[error("package `{0}` does not exist")]
	PackageDNE(String),

	#[error("failed to install package: {0}")]
	PackageInstall(i64),

	#[error("failed to update system: {0}")]
	SystemUpdate(i64),
}

#[derive(Default)]
pub struct Context {
	docker: Option<Docker>,
	container_id: String,
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
	pub fn connect(&mut self) -> Result<(), Error> {
		self.docker = Some(Docker::connect_with_defaults().map_err(Error::Connection)?);
		return Ok(());
	}

	fn get_docker(&self) -> Result<&Docker, Error> {
		return self.docker.as_ref().ok_or(Error::NotConnected);
	}

	async fn install_packages(&self, spinner: &ProgressBar, current_task: usize, total_tasks: usize, packages: &str) -> Result<(), Error> {
		for (i, pkg) in packages.split_whitespace().enumerate() {
			spinner.set_message(format!("Installing {pkg}"));
			spinner.set_prefix(format!("[{}/{total_tasks}]", i + current_task));
			let exec_id = self
				.create_exec(format!("/bin/pacman -Ssq \"^{pkg}$\""), false)
				.await?;
			let status = self.start_exec(&exec_id, false).await?;
			if status != 0 {
				return Err(Error::PackageDNE(pkg.to_string()));
			}
			let exec_id = self
				.create_exec(format!("/bin/sudo /bin/pacman -S --needed --noconfirm {pkg}"), false)
				.await?;
			let status = self.start_exec(&exec_id, false).await?;
			if status != 0 {
				return Err(Error::PackageInstall(status));
			}
		}

		return Ok(());
	}

	async fn install_aur_packages(&self, spinner: &ProgressBar, current_task: usize, total_tasks: usize, packages: &str) -> Result<(), Error> {
		for (i, pkg) in packages.split_whitespace().enumerate() {
			spinner.set_message(format!("Installing {pkg} from AUR"));
			spinner.set_prefix(format!("[{}/{total_tasks}]", i + current_task));
			let exec_id = self
				.create_exec(format!("/bin/yay --aur -Ssq \"^{pkg}$\""), false)
				.await?;
			let status = self.start_exec(&exec_id, false).await?;
			if status != 0 {
				return Err(Error::PackageDNE(pkg.to_string()));
			}
			let exec_id = self
				.create_exec(format!("/bin/yay --sync --needed --noconfirm --noprogressbar {pkg}"), false)
				.await?;
			let status = self.start_exec(&exec_id, false).await?;
			if status != 0 {
				return Err(Error::PackageInstall(status));
			}
		}

		return Ok(());
	}

	async fn update_system(&self) -> Result<(), Error> {
		let exec_id = self
			.create_exec("/bin/sudo /bin/pacman -Syu --noconfirm".into(), false)
			.await?;
		let status = self.start_exec(&exec_id, false).await?;
		if status != 0 {
			return Err(Error::SystemUpdate(status));
		}

		return Ok(());
	}

	pub async fn perform_all_enter(&mut self, args: &Args) -> Result<i64, Error> {
		let m = MultiProgress::new();
		let total = 5
			+ args
				.extra_packages
				.as_ref()
				.unwrap_or(&"".to_string())
				.split_whitespace()
				.count() + args
			.extra_aur_packages
			.as_ref()
			.unwrap_or(&"".to_string())
			.split_whitespace()
			.count() + args.update_system as usize;
		let spinner = m.add(ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}...").unwrap()));
		{
			spinner.set_message("Downloading image");
			spinner.set_prefix(format!("[1/{total}]"));
			spinner.enable_steady_tick(Duration::from_millis(50));
			self.pull_image(&m).await?;
		}
		self.container_id = {
			spinner.set_message("Creating system");
			spinner.set_prefix(format!("[2/{total}]"));
			self.create_container(args.no_network, args.privileged, args.ro_root, args.ro_cwd, !args.disable_cwd_mount)
				.await?
		};
		{
			spinner.set_message("Starting system");
			spinner.set_prefix(format!("[3/{total}]"));
			self.start_container().await?;
		}
		if args.update_system {
			spinner.set_message("Updating system");
			spinner.set_prefix(format!("[4/{total}]"));
			self.update_system().await?;
		}
		if let Some(pkgs) = &args.extra_packages {
			self.install_packages(&spinner, 4 + args.update_system as usize, total, pkgs)
				.await?;
		}
		if let Some(pkgs) = &args.extra_aur_packages {
			self.install_aur_packages(&spinner, 5 + args.update_system as usize, total, pkgs)
				.await?;
		}
		let exec_id = {
			spinner.set_message("Executing");
			spinner.set_prefix(format!("[{}/{total}]", total - 1));
			if args.command.len() == 1 && args.command[0] == "/usr/bin/zsh" {
				self.create_exec("SHOW_WELCOME=true /usr/bin/zsh".into(), true)
					.await?
			} else {
				self.create_exec(
					args.command
						.iter()
						.map(|s| s.escape_default().to_string())
						.collect::<Vec<String>>()
						.join(" "),
					true,
				)
				.await?
			}
		};
		spinner.finish_and_clear();
		m.remove(&spinner);
		let exit_code = self.start_exec(&exec_id, true).await?;

		let spinner = m.add(ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}...").unwrap()));
		{
			spinner.set_message("Deleting system");
			spinner.set_prefix(format!("[{total}/{total}]"));
			spinner.enable_steady_tick(Duration::from_millis(50));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.delete_container().await?;
		}
		spinner.finish_and_clear();
		m.remove(&spinner);
		return Ok(exit_code);
	}

	pub async fn delete_container(&self) -> Result<(), Error> {
		let docker = self.get_docker()?;
		docker
			.remove_container(
				&self.container_id,
				Some(
					bollard::query_parameters::RemoveContainerOptionsBuilder::default()
						.force(true)
						.build(),
				),
			)
			.await
			.map_err(Error::ContainerDelete)?;

		return Ok(());
	}

	async fn create_exec(&self, command: String, attach: bool) -> Result<String, Error> {
		let docker = self.get_docker()?;
		let exec = docker
			.create_exec(
				&self.container_id,
				bollard::models::ExecConfig {
					attach_stdout: Some(attach),
					attach_stderr: Some(attach),
					attach_stdin: Some(attach),
					user: Some("tempsystem".into()),
					tty: Some(attach),
					cmd: Some(vec!["/usr/bin/zsh".into(), "-c".into(), format!("{command}")]),
					..Default::default()
				},
			)
			.await
			.map_err(Error::ExecCreate)?
			.id;
		return Ok(exec);
	}

	async fn start_exec(&self, exec_id: &str, attach: bool) -> Result<i64, Error> {
		let docker = self.get_docker()?;
		if attach {
			let (mut output, mut input) = if let bollard::exec::StartExecResults::Attached { output, input } = docker
				.start_exec(exec_id, None)
				.await
				.map_err(Error::ExecStart)?
			{
				(output, input)
			} else {
				return Err(Error::ExpectedAttached);
			};
			tokio::task::spawn(async move {
				#[allow(clippy::unbuffered_bytes)]
				let mut stdin = async_stdin().bytes();
				loop {
					if let Some(Ok(byte)) = stdin.next()
						&& let Err(e) = input.write_all(&[byte]).await
					{
						print_error!("failed to write to exec's stdin", e);
						break;
					} else {
						tokio::time::sleep(Duration::from_nanos(10)).await;
					}
				}
			});

			let tty_size = terminal_size().map_err(Error::TerminalSize)?;
			docker
				.resize_exec(
					exec_id,
					bollard::query_parameters::ResizeExecOptionsBuilder::default()
						.h(tty_size.1 as i32)
						.w(tty_size.0 as i32)
						.build(),
				)
				.await
				.map_err(Error::ExecResize)?;

			let stdout = std::io::stdout();
			let mut stdout = stdout.lock().into_raw_mode().map_err(Error::Rawmode)?;

			while let Some(Ok(output)) = output.next().await {
				stdout
					.write_all(output.into_bytes().as_ref())
					.map_err(Error::StdoutWrite)?;
				stdout.flush().map_err(Error::StdoutFlush)?;
			}
		} else if let bollard::exec::StartExecResults::Detached = docker
			.start_exec(exec_id, None)
			.await
			.map_err(Error::ExecStart)?
		{
			return Err(Error::ExpectedDetached);
		} else {
			loop {
				let inspect = docker
					.inspect_exec(exec_id)
					.await
					.map_err(Error::ExecInspect)?;
				if !inspect.running.unwrap() {
					break;
				}
				tokio::time::sleep(Duration::from_millis(300)).await;
			}
		}

		let inspect = docker
			.inspect_exec(exec_id)
			.await
			.map_err(Error::ExecInspect)?;
		return Ok(inspect.exit_code.unwrap_or(0));
	}

	async fn pull_image(&self, m: &MultiProgress) -> Result<(), Error> {
		let docker = self.get_docker()?;
		let mut stream = docker.create_image(
			Some(
				bollard::query_parameters::CreateImageOptionsBuilder::default()
					.from_image("codeberg.org/land/tempsystem:latest")
					.build(),
			),
			None,
			None,
		);
		let sty = ProgressStyle::with_template("[{elapsed_precise}] {bar:40.cyan/blue} {bytes:>15}/{total_bytes:15} {msg}")
			.unwrap()
			.progress_chars("##-");
		let mut bars: HashMap<String, ProgressBar> = HashMap::new();
		while let Some(update) = stream.next().await {
			let update = update.map_err(Error::ImageCreate)?;
			if let Some(id) = update.id
				&& id != "latest"
			{
				if let Some(progress) = update.progress_detail
					&& let Some(cur) = progress.current
					&& let Some(total) = progress.total
				{
					let pb = bars
						.entry(id.clone())
						.or_insert(m.add(ProgressBar::new(total as u64).with_style(sty.clone())));
					pb.set_position(cur as u64);
					pb.set_length(total as u64);
				}
				let pb = bars
					.entry(id.clone())
					.or_insert(m.add(ProgressBar::no_length().with_style(sty.clone())));
				if let Some(msg) = update.status {
					if msg == "Pull complete" {
						pb.finish_and_clear();
						m.remove(pb);
						bars.remove(&id);
					} else if msg.ends_with(" complete")
						&& let Some(max) = pb.length()
					{
						pb.set_position(max);
					} else {
						pb.set_message(msg);
					}
				}
			}
		}
		for (_, pb) in bars {
			pb.finish_and_clear();
		}

		return Ok(());
	}

	async fn create_container(&self, network_disabled: bool, privileged: bool, ro_root: bool, ro_cwd: bool, mount_cwd: bool) -> Result<String, Error> {
		let docker = self.get_docker()?;
		let binds = if mount_cwd {
			if ro_cwd {
				vec![format!("{}:/home/tempsystem/work:ro", std::env::current_dir().map_err(Error::GetCWD)?.display())]
			} else {
				vec![format!("{}:/home/tempsystem/work", std::env::current_dir().map_err(Error::GetCWD)?.display())]
			}
		} else {
			vec![]
		};
		let id = docker
			.create_container(
				None::<bollard::query_parameters::CreateContainerOptions>,
				bollard::models::ContainerCreateBody {
					image: Some("codeberg.org/land/tempsystem:latest".to_string()),
					tty: Some(true),
					hostname: Some("tempsystem".into()),
					network_disabled: Some(network_disabled),
					host_config: Some(bollard::secret::HostConfig {
						dns: Some(vec!["1.1.1.1".into(), "1.0.0.1".into()]),
						privileged: Some(privileged),
						readonly_rootfs: Some(ro_root),
						binds: Some(binds),
						..Default::default()
					}),
					..Default::default()
				},
			)
			.await
			.map_err(Error::ContainerCreate)?
			.id;

		return Ok(id);
	}

	async fn start_container(&self) -> Result<(), Error> {
		let docker = self.get_docker()?;
		docker
			.start_container(&self.container_id, None::<bollard::query_parameters::StartContainerOptions>)
			.await
			.map_err(Error::ContainerStart)?;

		return Ok(());
	}
}
