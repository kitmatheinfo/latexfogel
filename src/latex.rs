use crate::pdf;

pub fn render_to_png(input: &str) -> anyhow::Result<Vec<u8>> {
    let latex = r"
        \documentclass[preview]{standalone}
        \usepackage[a5paper]{geometry}
        \usepackage{amsmath,amssymb}
        \usepackage{xcolor}
        \definecolor{discordbg}{HTML}{313338}
        \begin{document}
        \color{white}
        \pagecolor{discordbg}
        {{input}}
        \end{document}
    "
    .replace("{{inpu}}", input);

    pdf::pdf_to_png(pdf::render_pdf(&latex)?)
}
