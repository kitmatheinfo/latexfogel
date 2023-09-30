use std::path::Path;

mod pdf;

fn main() {
    let latex = r"
        \documentclass[preview]{standalone}
        \usepackage[a5paper]{geometry}
        \usepackage{amsmath,amssymb}
        \usepackage{xcolor}
        \definecolor{discordbg}{HTML}{313338}
        \begin{document}
        \color{white}
        \pagecolor{discordbg}
        Dies ist ein text, text, text, text, text, text, text,text, text, text, text, text, text,
         text,text, text, text, text, text, text, text,text, text, text, text, text, text, text,
         text, text, text, text, text, text, text,text, text, text, text, text, text, text,text,
         text, text, text, text, text, text,text, text, text, text, text, text, text,text, text,
         text, text, text, text, text,text, text, text, text, text, text, text,text, text, text,
         text, text, text, text,text, text, text, text, text, text, text,
         $\displaystyle \frac{1}{\sum_{i = 1}^{20} 2 + 5}$
        \end{document}
    ";

    let pdf = pdf::render_pdf(latex).unwrap();
    let png = pdf::pdf_to_png(pdf).unwrap();

    std::fs::write(Path::new("/tmp/foo.png"), png).unwrap();
}
