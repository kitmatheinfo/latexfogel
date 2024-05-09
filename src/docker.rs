//! Run latexfogel subcommands in a docker image.

use std::{
    ffi::OsString,
    process::{Output, Stdio},
    time::Duration,
};

use anyhow::bail;
use log::info;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
    time,
};

pub struct DockerCommand {
    image: String,
    name: String,
    args: Vec<OsString>,
}

impl DockerCommand {
    pub fn new(image: String, name: String) -> Self {
        Self {
            image,
            name,
            args: vec![],
        }
    }

    pub fn arg<S: Into<OsString>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    pub async fn run(self, input: &str) -> anyhow::Result<Output> {
        pull_docker_image(&self.image).await?;

        let child = spawn_runner(&self.name, &self.image, &self.args, input).await?;

        let output = match time::timeout(Duration::from_secs(15), child.wait_with_output()).await {
            Ok(output) => output?,
            Err(_elapsed) => {
                info!("Runner {:?} timed out, killing it", self.name);
                kill_runner(&self.name).await?;
                bail!("Timeout reached")
            }
        };

        if !output.status.success() {
            bail!(
                "Runner died with {}\nStdout:\n{}\nStderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        }

        Ok(output)
    }
}

async fn pull_docker_image(image: &str) -> anyhow::Result<()> {
    info!("Pulling image: {image:?}");

    let output = Command::new("docker")
        .arg("pull")
        .arg(image)
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "Failed to pull runner image {image:?}\nStdout:\n{}\nStderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    info!("Pulled image");
    Ok(())
}

async fn spawn_runner(
    name: &str,
    image: &str,
    args: &[OsString],
    input: &str,
) -> anyhow::Result<Child> {
    let mut child = Command::new("docker")
        .arg("run")
        .arg("--pids-limit=5000")
        .arg("--memory=500M")
        .arg("--cpus=1")
        .arg("--interactive=true")
        .arg("--read-only")
        .arg("--network=none")
        .arg("--cap-drop=all")
        .arg("--tmpfs=/tmp")
        .arg(format!("--name={name}"))
        .arg("--rm")
        .arg(image)
        .args(args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .await?;

    Ok(child)
}

async fn kill_runner(name: &str) -> anyhow::Result<()> {
    let output = tokio::process::Command::new("docker")
        .arg("kill")
        .arg(name)
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "Failed to kill runner {name:?}\nStdout:\n{}\nStderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    Ok(())
}
