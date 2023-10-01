use std::cell::RefCell;
use std::fmt::Arguments;
use std::process::Command;
use std::rc::Rc;

use anyhow::anyhow;
use tectonic::driver::{OutputFormat, PassSetting, ProcessingSession, ProcessingSessionBuilder};
use tectonic::status::{MessageKind, StatusBackend};
use tectonic::{tt_error, Error, ErrorKind};
use tectonic_bridge_core::{SecuritySettings, SecurityStance};

const FILE_NAME: &str = "foo";

#[derive(Default)]
struct StatusBackendState {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl StatusBackendState {
    fn overrun_hbox(&self) -> bool {
        self.warnings
            .iter()
            .any(|it| it.contains(r"Overfull \hbox"))
    }
}

#[derive(Default)]
struct StringStatusBackend(Rc<RefCell<StatusBackendState>>);

impl StatusBackend for StringStatusBackend {
    fn report(&mut self, kind: MessageKind, args: Arguments, err: Option<&anyhow::Error>) {
        if kind == MessageKind::Warning {
            self.0.borrow_mut().warnings.push(format!("{args}"));
        }
        if kind != MessageKind::Error {
            return;
        }
        if let Some(err) = err {
            self.0.borrow_mut().errors.push(format!("{args}: {err}"));
        } else {
            self.0.borrow_mut().errors.push(format!("{args}"));
        }
    }

    fn dump_error_logs(&mut self, output: &[u8]) {
        let foo = String::from_utf8_lossy(output);
        self.0.borrow_mut().errors.push(foo.to_string());
    }
}

pub struct PdfResult {
    pub pdf: Vec<u8>,
    pub overrun_hbox: bool,
}

pub fn render_pdf(latex: &str) -> anyhow::Result<PdfResult> {
    let status_state = Rc::new(RefCell::new(StatusBackendState::default()));
    let status_backend = StringStatusBackend(status_state.clone());
    let mut status_backend = Box::new(status_backend) as Box<dyn StatusBackend>;
    let security = SecuritySettings::new(SecurityStance::DisableInsecures);
    let mut session_builder = ProcessingSessionBuilder::new_with_security(security);
    let bundle = tectonic_bundles::get_fallback_bundle(
        tectonic_engine_xetex::FORMAT_SERIAL,
        false,
        &mut *status_backend,
    )?;
    session_builder
        .format_name("latex")
        .tex_input_name(&format!("{FILE_NAME}.tex"))
        .bundle(Box::new(bundle))
        .keep_logs(false)
        .keep_intermediates(false)
        .synctex(false)
        .output_format(OutputFormat::Pdf)
        .pass(PassSetting::Default)
        .primary_input_buffer(latex.as_bytes())
        .do_not_write_output_files()
        .print_stdout(false)
        .build_date_from_env(true);

    let mut session = create_session(session_builder, &mut status_backend)?;
    let result = session.run(&mut *status_backend);

    if let Err(e) = &result {
        return handle_error(status_state, &mut status_backend, &mut session, e);
    }

    return match session.into_file_data().get(&format!("{FILE_NAME}.pdf")) {
        Some(file) => Ok(PdfResult {
            pdf: file.data.clone(),
            overrun_hbox: status_state.borrow().overrun_hbox(),
        }),
        None => Err(anyhow!("Got no output file")),
    };
}

fn create_session(
    session_builder: ProcessingSessionBuilder,
    status_backend: &mut Box<dyn StatusBackend>,
) -> anyhow::Result<ProcessingSession> {
    let session = session_builder.create(&mut **status_backend);

    match session {
        Ok(s) => Ok(s),
        Err(e) => Err(anyhow!(format!("Oh no: {e}"))),
    }
}

fn handle_error(
    status_string: Rc<RefCell<StatusBackendState>>,
    status_backend: &mut Box<dyn StatusBackend>,
    session: &mut ProcessingSession,
    e: &Error,
) -> anyhow::Result<PdfResult> {
    if let ErrorKind::EngineError(engine) = e.kind() {
        let output = session.get_stdout_content();

        if output.is_empty() {
            tt_error!(
                status_backend,
                "something bad happened inside {}, but no output was logged",
                engine
            );
        } else {
            tt_error!(
                status_backend,
                "something bad happened inside {}; its output follows:\n",
                engine
            );
            status_backend.dump_error_logs(&output);
        }
    }
    Err(anyhow!(
        "**{}**\n```\n{}\n```",
        e.kind(),
        status_string.borrow().errors.join("\n")
    ))
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
        return Err(anyhow!(
            "Error running command: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(std::fs::read(png_path)?)
}
