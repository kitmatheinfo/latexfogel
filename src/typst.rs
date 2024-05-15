use std::{
    fs,
    io::{ErrorKind, Read, Write},
    path::PathBuf,
    sync::OnceLock,
};

use anyhow::anyhow;
use comemo::Prehashed;
use typst::{
    diag::{FileError, FileResult},
    eval::Tracer,
    foundations::{Bytes, Datetime},
    layout::Abs,
    syntax::{FileId, Source},
    text::{Font, FontBook, FontInfo},
    visualize::Color,
    Library, World,
};

use crate::docker::DockerCommand;

// The logic for detecting and loading fonts was ripped straight from:
// https://github.com/typst/typst/blob/69dcc89d84176838c293b2d59747cd65e28843ad/crates/typst-cli/src/fonts.rs
// https://github.com/typst/typst/blob/69dcc89d84176838c293b2d59747cd65e28843ad/crates/typst-cli/src/world.rs#L193-L195

struct FontSlot {
    path: PathBuf,
    index: u32,
    font: OnceLock<Option<Font>>,
}

impl FontSlot {
    pub fn get(&self) -> Option<Font> {
        self.font
            .get_or_init(|| {
                let data = fs::read(&self.path).ok()?.into();
                Font::new(data, self.index)
            })
            .clone()
    }
}

struct FontLoader {
    book: FontBook,
    fonts: Vec<FontSlot>,
}

impl FontLoader {
    fn new() -> Self {
        Self {
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    fn load_embedded_fonts(&mut self) {
        // https://github.com/typst/typst/blob/be12762d942e978ddf2e0ac5c34125264ab483b7/crates/typst-cli/src/fonts.rs#L107-L121
        for font_file in typst_assets::fonts() {
            let font_data = Bytes::from_static(font_file);
            for (i, font) in Font::iter(font_data).enumerate() {
                self.book.push(font.info().clone());
                self.fonts.push(FontSlot {
                    path: PathBuf::new(),
                    index: i as u32,
                    font: OnceLock::from(Some(font)),
                });
            }
        }
    }

    fn load_system_fonts(&mut self) {
        // https://github.com/typst/typst/blob/be12762d942e978ddf2e0ac5c34125264ab483b7/crates/typst-cli/src/fonts.rs#L70-L100
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        for face in db.faces() {
            let path = match &face.source {
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path,
                fontdb::Source::Binary(_) => continue,
            };

            if let Some(info) = db.with_face_data(face.id, FontInfo::new).unwrap() {
                self.book.push(info);
                self.fonts.push(FontSlot {
                    path: path.clone(),
                    index: face.index,
                    font: OnceLock::new(),
                })
            }
        }
    }
}

struct DummyWorld {
    library: Prehashed<Library>,
    book: Prehashed<FontBook>,
    main: Source,
    fonts: Vec<FontSlot>,
}

impl DummyWorld {
    fn new(main: String) -> Self {
        let mut loader = FontLoader::new();
        loader.load_embedded_fonts();
        loader.load_system_fonts();

        Self {
            library: Prehashed::new(Library::builder().build()),
            book: Prehashed::new(loader.book),
            main: Source::detached(main),
            fonts: loader.fonts,
        }
    }
}

impl World for DummyWorld {
    fn library(&self) -> &Prehashed<Library> {
        &self.library
    }

    fn book(&self) -> &Prehashed<FontBook> {
        &self.book
    }

    fn main(&self) -> Source {
        self.main.clone()
    }

    fn source(&self, _id: FileId) -> FileResult<Source> {
        Err(FileError::AccessDenied)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let Some(package) = id.package() else {
            return Err(FileError::Other(Some(
                "only packages can be imported".into(),
            )));
        };

        let mut path: PathBuf = std::env::var("TYPST_PACKAGES")
            .map_err(|_| FileError::Other(Some("can't find my packages D:".into())))?
            .into();

        // Translate package spec to path in packages repo.
        // https://github.com/typst/packages/tree/main?tab=readme-ov-file#published-packages
        path.push(package.namespace.as_str());
        path.push(package.name.as_str());
        path.push(package.version.to_string());
        path.push(id.vpath().as_rootless_path());

        let file = fs::read(&path).map_err(|e| match e.kind() {
            ErrorKind::NotFound => FileError::NotFound(path),
            _ => FileError::AccessDenied,
        })?;

        Ok(file.into())
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts[index].get()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

pub fn render_to_png(typst: String) -> anyhow::Result<Vec<u8>> {
    let typst = [
        "#set page(width: 11.5cm, height: auto, margin: (x: 1mm, y: 2mm))",
        "#set page(fill: rgb(\"#313338\"))", // Discord background color
        "#set text(white)",
        &typst,
    ]
    .join("\n");

    let world = DummyWorld::new(typst);
    let mut tracer = Tracer::new();

    let document = typst::compile(&world, &mut tracer).map_err(|errs| {
        // Errors could be nicer, e.g.
        // https://github.com/typst/typst/blob/be12762d942e978ddf2e0ac5c34125264ab483b7/crates/typst-cli/src/compile.rs#L461-L501
        let errs = errs
            .into_iter()
            .map(|sd| sd.message.to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
        anyhow!("Failed to compile typst code:\n\n{errs}")
    })?;

    // Color doesn't matter, it is already set by the document itself
    let png = typst_render::render_merged(&document, 4.0, Color::WHITE, Abs::zero(), Color::WHITE)
        .encode_png()?;

    Ok(png)
}

pub struct RenderedTypst {
    pub png: Vec<u8>,
}

pub fn run_renderer() {
    let mut typst = String::new();
    std::io::stdin()
        .read_to_string(&mut typst)
        .expect("could not read stdin");

    match render_to_png(typst) {
        Ok(png) => {
            std::io::stdout()
                .write_all(&png)
                .expect("could not write image");
        }
        Err(err) => {
            eprintln!("Error rendering typst: {err}");
            std::process::exit(1);
        }
    }
}

pub async fn render_typst(
    context_id: u64,
    renderer_image: String,
    typst: String,
) -> anyhow::Result<RenderedTypst> {
    let output = DockerCommand::new(renderer_image, format!("slave-typst-{context_id}"))
        .arg("render-typst")
        .run(&typst)
        .await?;

    let png = output.stdout.to_vec();
    Ok(RenderedTypst { png })
}
