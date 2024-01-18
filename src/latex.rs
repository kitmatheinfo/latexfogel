use std::io::{Read, Write};
use std::process::Stdio;
use std::time::Duration;

use anyhow::bail;
use log::{error, info};
use time::timeout;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::time;

use crate::pdf;

pub enum ImageWidth {
    Wide,
    Normal,
}

impl ImageWidth {
    fn width(&self) -> &'static str {
        match self {
            ImageWidth::Wide => "18cm",
            ImageWidth::Normal => "11.5cm",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            ImageWidth::Wide => "wide",
            ImageWidth::Normal => "normal",
        }
    }
}

impl TryFrom<&str> for ImageWidth {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            "wide" => Ok(Self::Wide),
            "normal" => Ok(Self::Normal),
            _ => Err(()),
        }
    }
}

pub struct PngResult {
    pub png: Vec<u8>,
    pub overrun_hbox: bool,
}

async fn render_to_png(width: ImageWidth, input: &str) -> anyhow::Result<PngResult> {
    let latex = r"
        \documentclass[preview,border=2pt]{standalone}
        \usepackage[paperwidth={{width}},paperheight=21cm,top=0mm,bottom=0mm,left=0mm,right=0mm]{geometry}
        \usepackage{amsmath,amssymb}
        \usepackage{xcolor}
        \usepackage{bussproofs}
        \definecolor{discordbg}{HTML}{313338}
        \begin{document}
        \color{white}
        \pagecolor{discordbg}
        {{input}}
        \end{document}
    "
        .replace("{{input}}", input)
        .replace("{{width}}", width.width());

    let pdf_result = pdf::render_pdf(&latex).await?;
    Ok(PngResult {
        png: pdf::pdf_to_png(pdf_result.pdf)?,
        overrun_hbox: pdf_result.overrun_hbox,
    })
}

pub async fn run_renderer() {
    info!("Pivoting to tmp dir: {:?}", std::env::temp_dir());
    std::env::set_current_dir(std::env::temp_dir()).expect("could not change to tempdir");

    let mut width = String::new();
    std::io::stdin()
        .read_line(&mut width)
        .expect("could not read width");

    let width: ImageWidth = (*width)
        .try_into()
        .unwrap_or_else(|_| panic!("could not parse width: '{width}'"));

    let mut latex = String::new();
    std::io::stdin()
        .read_to_string(&mut latex)
        .expect("could not read stdin");

    match render_to_png(width, &latex).await {
        Ok(result) => {
            std::io::stdout()
                .write_all(&[0])
                .expect("write error failed");
            std::io::stdout()
                .write_all(&[if result.overrun_hbox { 1 } else { 0 }])
                .expect("write error failed");
            std::io::stdout()
                .write_all(&result.png)
                .expect("could not write image");
        }
        Err(err) => {
            std::io::stdout()
                .write_all(&[1])
                .expect("write error failed");
            println!("{err}");
        }
    }
}

pub async fn render_latex(
    context_id: u64,
    renderer_image: &str,
    latex: String,
    width: ImageWidth,
) -> anyhow::Result<PngResult> {
    info!("Pulling image");
    pull_renderer_image(renderer_image).await?;
    info!("Pulled image");

    let child = spawn_renderer_process(context_id, renderer_image, latex, width).await?;

    let output = timeout(Duration::from_secs(15), child.wait_with_output()).await;
    let output = match output {
        Ok(out) => out?,
        Err(_) => {
            info!("Renderer {context_id} timed out, killing it");
            kill_renderer_process(context_id).await?;
            bail!("Timeout reached.");
        }
    };

    if !output.status.success() {
        error!(
            "Renderer died with {}.\nStdout:{}\nStderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!("Renderer exited with non-zero exit code {}", output.status);
    }

    if output.stdout.len() < 3 {
        error!(
            "Renderer output too short with {}.\nStdout:{}\nStderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!("Renderer output not long enough");
    }

    let error_bit = output.stdout[0];
    if error_bit == 1 {
        let stdout = String::from_utf8_lossy(&output.stdout.as_slice()[1..]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        info!("Render failed:\nStdout:\n{stdout}\nStderr:\n{stderr}");
        bail!("{}", stdout);
    }
    let overflow_bit = output.stdout[1];
    let overrun_hbox = overflow_bit != 0;
    let png = output.stdout.as_slice()[2..].to_vec();

    Ok(PngResult { overrun_hbox, png })
}

async fn pull_renderer_image(renderer_image: &str) -> anyhow::Result<()> {
    let child = tokio::process::Command::new("docker")
        .arg("pull")
        .arg(renderer_image)
        .output()
        .await?;

    if !child.status.success() {
        bail!(
            "Could not pull renderer image.\nStdout:\n{}\nStderr:\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        )
    }
    Ok(())
}

async fn spawn_renderer_process(
    context_id: u64,
    renderer_image: &str,
    latex: String,
    width: ImageWidth,
) -> anyhow::Result<Child> {
    let mut child = tokio::process::Command::new("docker")
        .arg("run")
        .arg("--pids-limit=2000")
        .arg("--memory=100M")
        .arg("--cpus=1")
        .arg("--interactive=true")
        .arg("--read-only")
        .arg("--network=none")
        .arg("--cap-drop=all")
        .arg("--tmpfs=/tmp")
        .arg(format!("--name=slave-{context_id}"))
        .arg("--rm")
        .arg(renderer_image)
        .arg("renderer")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    // pass latex
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(format!("{}\n", width.name()).as_bytes())
        .await?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(latex.as_bytes())
        .await?;
    Ok(child)
}

async fn kill_renderer_process(context_id: u64) -> anyhow::Result<()> {
    let output = tokio::process::Command::new("docker")
        .arg("kill")
        .arg(format!("slave-{context_id}"))
        .output()
        .await?;

    if !output.status.success() {
        error!(
            "Failed to kill renderer: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        bail!("could not kill renderer")
    } else {
        Ok(())
    }
}
