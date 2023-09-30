use std::cell::RefCell;
use std::fmt::Arguments;
use std::process::Command;
use std::rc::Rc;

use anyhow::anyhow;
use tectonic::{Error, ErrorKind, tt_error};
use tectonic::driver::{OutputFormat, PassSetting, ProcessingSession, ProcessingSessionBuilder};
use tectonic::status::{MessageKind, StatusBackend};
use tectonic_bridge_core::{SecuritySettings, SecurityStance};

const FILE_NAME: &str = "foo";

#[derive(Default)]
struct StringStatusBackend {
    output: Rc<RefCell<String>>,
}

impl StatusBackend for StringStatusBackend {
    fn report(&mut self, kind: MessageKind, args: Arguments, err: Option<&anyhow::Error>) {
        if kind != MessageKind::Error {
            return;
        }
        if let Some(err) = err {
            *self.output.borrow_mut() = format!("{}\n{}: {}", self.output.borrow(), args, err);
        } else {
            *self.output.borrow_mut() = format!("{}\n{}", self.output.borrow(), args)
        }
    }

    fn dump_error_logs(&mut self, output: &[u8]) {
        if let Ok(output) = String::from_utf8(Vec::from(output)) {
            *self.output.borrow_mut() = format!("{}\n{}", self.output.borrow(), output);
        } else {
            *self.output.borrow_mut() = format!("{}\nInvalid utf-8", self.output.borrow());
        }
    }
}

pub fn render_pdf(latex: &str) -> anyhow::Result<Vec<u8>> {
    let status_string = Rc::new(RefCell::new(String::new()));
    let status_backend = StringStatusBackend { output: status_string.clone() };
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
        return handle_error(status_string, &mut status_backend, &mut session, e);
    }

    return match session.into_file_data().get(&format!("{FILE_NAME}.pdf")) {
        Some(file) => Ok(file.data.clone()),
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
    status_string: Rc<RefCell<String>>,
    status_backend: &mut Box<dyn StatusBackend>,
    session: &mut ProcessingSession,
    e: &Error,
) -> anyhow::Result<Vec<u8>> {
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
    Err(anyhow!(format!("**{}**\n```\n{}\n```", e.kind(), status_string.borrow().trim())))
}

pub fn pdf_to_png(pdf: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let dir = tempfile::tempdir()?;
    let pdf_path = dir.path().join("foo.pdf");
    let png_path = dir.path().join("foo.png");

    std::fs::write(&pdf_path, pdf)?;
    Command::new("inkscape")
        .arg(pdf_path.to_str().unwrap())
        .arg("-o")
        .arg(png_path.to_str().unwrap())
        .arg("--export-dpi=300")
        .output()?;

    let png_data = std::fs::read(png_path)?;
    Ok(png_data)
}
