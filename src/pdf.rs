use std::process::Command;

use anyhow::bail;
use log::error;

pub struct PdfResult {
    pub pdf: Vec<u8>,
    pub overrun_hbox: bool,
}
pub async fn render_pdf(latex: &str) -> anyhow::Result<PdfResult> {
    let tempdir = tempfile::tempdir()?;
    let latex_path = tempdir.path().join("foo.tex");
    std::fs::write(&latex_path, latex)?;

    let out = tokio::process::Command::new("latexmk")
        .current_dir(tempdir.path())
        .arg("-interaction=nonstopmode")
        .arg("-halt-on-error")
        .arg("-pdf")
        .arg(latex_path.to_str().unwrap())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&out.stdout);

    if out.status.success() {
        return Ok(PdfResult {
            pdf: std::fs::read(latex_path.with_extension("pdf"))?,
            overrun_hbox: stdout.contains(r"Overfull \hbox"),
        });
    }

    if let Some(error_line) = stdout
        .lines()
        .find(|line| line.contains("Fatal error occurred"))
    {
        let prefix = error_line.split_once(' ').unwrap().0;

        if let Some(error) = stdout.lines().find(|line| line.starts_with(prefix)) {
            let error = error.strip_prefix(prefix).unwrap().trim();
            bail!("**Invalid LaTeX**\n```\n{error}\n```")
        }
        bail!("**Your LaTeX code contains an unknown error**");
    }

    let stderr = String::from_utf8_lossy(&out.stderr);

    error!("LaTeX output\nStdout:\n{}\nStderr: {}", stdout, stderr);

    bail!("**Unknown error**\n```{stderr}```");
}
pub fn pdf_to_png(pdf: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let dir = tempfile::tempdir()?;
    let pdf_path = dir.path().join("foo.pdf");
    let png_path = dir.path().join("foo.png");

    std::fs::write(&pdf_path, pdf)?;
    let out = Command::new("magick")
        .arg("-density")
        .arg("300")
        .arg(pdf_path.to_str().unwrap())
        .arg(png_path.to_str().unwrap())
        .output()?;
    if !out.status.success() {
        bail!(
            "Error running pdf->png conversion:\nStdout:\n{}\nStderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
    Ok(std::fs::read(png_path)?)
}
