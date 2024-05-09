use std::io::{Read, Write};

use anyhow::bail;
use log::{error, info};

use crate::docker::DockerCommand;
use crate::{pdf, ImageWidth};

fn image_width_measure(width: ImageWidth) -> &'static str {
    match width {
        ImageWidth::Wide => "18cm",
        ImageWidth::Normal => "11.5cm",
    }
}

pub struct RenderedLatex {
    pub png: Vec<u8>,
    pub overrun_hbox: bool,
}

async fn render_to_png(width: ImageWidth, input: &str) -> anyhow::Result<RenderedLatex> {
    let latex = r"
        \documentclass[preview,border=2pt]{standalone}
        \usepackage[paperwidth={{width}},paperheight=21cm,top=0mm,bottom=0mm,left=0mm,right=0mm]{geometry}
        \usepackage{fontspec}
        \usepackage{amsmath,amssymb}
        \usepackage{xcolor}
        \usepackage{bussproofs}
        \usepackage{braket}
        \usepackage{unicode-math}

        \setmathfont{Latin Modern Math}

        \definecolor{discordbg}{HTML}{313338}

        \begin{document}
        \color{white}
        \pagecolor{discordbg}
        {{input}}
        \end{document}
    "
        .replace("{{input}}", input)
        .replace("{{width}}", image_width_measure(width) );

    let pdf_result = pdf::render_pdf(&latex).await?;
    Ok(RenderedLatex {
        png: pdf::pdf_to_png(pdf_result.pdf)?,
        overrun_hbox: pdf_result.overrun_hbox,
    })
}

pub async fn run_renderer(width: ImageWidth) {
    info!("Pivoting to tmp dir: {:?}", std::env::temp_dir());
    std::env::set_current_dir(std::env::temp_dir()).expect("could not change to tempdir");

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
    renderer_image: String,
    latex: String,
    width: ImageWidth,
) -> anyhow::Result<RenderedLatex> {
    let output = DockerCommand::new(renderer_image, format!("slave-latex-{context_id}"))
        .arg("render-latex")
        .arg(width.arg_name())
        .run(&latex)
        .await?;

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

    Ok(RenderedLatex { png, overrun_hbox })
}
