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
}

pub struct PngResult {
    pub png: Vec<u8>,
    pub overrun_hbox: bool,
}

pub fn render_to_png(width: ImageWidth, input: &str) -> anyhow::Result<PngResult> {
    let latex = r"
        \documentclass[preview,border=2pt]{standalone}
        \usepackage[paperwidth={{width}},paperheight=21cm,top=0mm,bottom=0mm,left=0mm,right=0mm]{geometry}
        \usepackage{amsmath,amssymb}
        \usepackage{xcolor}
        \definecolor{discordbg}{HTML}{313338}
        \begin{document}
        \color{white}
        \pagecolor{discordbg}
        {{input}}
        \end{document}
    "
        .replace("{{input}}", input)
        .replace("{{width}}", width.width());

    let pdf_result = pdf::render_pdf(&latex)?;
    Ok(PngResult {
        png: pdf::pdf_to_png(pdf_result.pdf)?,
        overrun_hbox: pdf_result.overrun_hbox,
    })
}
