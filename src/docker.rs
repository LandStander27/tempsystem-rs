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

use crate::print_error;

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

	#[error("could not recv terminal size: {0}")]
	TerminalSize(std::io::Error),

	#[error("could not resize exec: {0}")]
	ExecResize(bollard::errors::Error),

	#[error("could not set raw mode: {0}")]
	Rawmode(std::io::Error),

	#[error("could not write to stdout: {0}")]
	StdoutWrite(std::io::Error),

	#[error("could flush stdout: {0}")]
	StdoutFlush(std::io::Error),

	#[error("could not delete container: {0}")]
	ContainerDelete(bollard::errors::Error),
}

// struct Tasks {
// 	pb: ProgressBar,
// 	total: usize,
// 	cur: usize,
// }

// impl Tasks {
// 	fn new(total: usize, pb: ProgressBar) -> Self {
// 		return Self { total, pb, cur: 0 };
// 	}

// 	fn start(&self) {
// 		self.pb.enable_steady_tick(Duration::from_millis(50));
// 	}

// 	async fn next(&mut self, msg: String) {
// 		self.cur += 1;
// 		self.pb.set_message(msg);
// 		self.pb.set_prefix(format!("[{}/{}]", self.cur, self.total));
// 		tokio::time::sleep(Duration::from_millis(250)).await;
// 	}

// 	fn finish(&self) {
// 		self.pb.finish_and_clear();
// 	}
// }

#[derive(Default)]
pub struct Context {
	docker: Option<Docker>,
}

impl Context {
	pub fn connect(&mut self) -> Result<(), Error> {
		self.docker = Some(Docker::connect_with_defaults().map_err(Error::Connection)?);
		return Ok(());
	}

	fn get_docker(&self) -> Result<&Docker, Error> {
		return self.docker.as_ref().ok_or(Error::NotConnected);
	}

	pub async fn perform_all_enter(&self) -> Result<(), Error> {
		let m = MultiProgress::new();
		let total = 5;
		let spinner = m.add(ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}...").unwrap()));
		{
			spinner.set_message("Downloading image");
			spinner.set_prefix(format!("[1/{total}]"));
			spinner.enable_steady_tick(Duration::from_millis(50));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.pull_image(&m).await?;
		}
		let id = {
			spinner.set_message("Creating system");
			spinner.set_prefix(format!("[2/{total}]"));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.create_container().await?
		};
		{
			spinner.set_message("Starting system");
			spinner.set_prefix(format!("[3/{total}]"));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.start_container(&id).await?;
		}
		let exec_id = {
			spinner.set_message("Executing");
			spinner.set_prefix(format!("[4/{total}]"));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.create_exec(&id, "SHOW_WELCOME=true /usr/bin/zsh".into())
				.await?
		};
		spinner.finish_and_clear();
		m.remove(&spinner);
		self.start_exec(&exec_id).await?;

		let spinner = m.add(ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}...").unwrap()));
		{
			spinner.set_message("Deleting system");
			spinner.set_prefix(format!("[5/{total}]"));
			spinner.enable_steady_tick(Duration::from_millis(50));
			tokio::time::sleep(Duration::from_millis(250)).await;
			self.delete_container(&id).await?;
		}
		spinner.finish_and_clear();
		m.remove(&spinner);
		return Ok(());
	}

	async fn delete_container(&self, id: &str) -> Result<(), Error> {
		let docker = self.get_docker()?;
		docker
			.remove_container(
				id,
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

	async fn create_exec(&self, id: &str, command: String) -> Result<String, Error> {
		let docker = self.get_docker()?;
		let exec = docker
			.create_exec(
				id,
				bollard::models::ExecConfig {
					attach_stdout: Some(true),
					attach_stderr: Some(true),
					attach_stdin: Some(true),
					tty: Some(true),
					cmd: Some(vec!["/usr/bin/zsh".into(), "-c".into(), format!("{command}")]),
					..Default::default()
				},
			)
			.await
			.map_err(Error::ExecCreate)?
			.id;
		return Ok(exec);
	}

	async fn start_exec(&self, exec_id: &str) -> Result<(), Error> {
		let docker = self.get_docker()?;
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

		return Ok(());
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

	async fn create_container(&self) -> Result<String, Error> {
		let docker = self.get_docker()?;
		let id = docker
			.create_container(
				None::<bollard::query_parameters::CreateContainerOptions>,
				bollard::models::ContainerCreateBody {
					image: Some("codeberg.org/land/tempsystem:latest".to_string()),
					tty: Some(true),
					..Default::default()
				},
			)
			.await
			.map_err(Error::ContainerCreate)?
			.id;

		return Ok(id);
	}

	async fn start_container(&self, id: &str) -> Result<(), Error> {
		let docker = self.get_docker()?;
		docker
			.start_container(id, None::<bollard::query_parameters::StartContainerOptions>)
			.await
			.map_err(Error::ContainerStart)?;

		return Ok(());
	}
}
