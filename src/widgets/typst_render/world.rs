use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::text::{Font, FontBook};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::diag::{FileResult, Severity};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// A resolved Typst diagnostic with byte positions and text.
pub struct TypstDiag {
    /// Diagnostic severity (error or warning).
    pub severity: Severity,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Additional hint strings.
    pub hints: Vec<String>,
    /// Byte range in the full source (including preamble).
    pub span_range: Option<std::ops::Range<usize>>,
}

const MATH_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/NewComputerModern/NewCMMath-Regular.otf"
));

const BODY_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/IosevkaGorbie/IosevkaGorbie-Regular.ttf"
));

const BODY_FONT_BOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/IosevkaGorbie/IosevkaGorbie-Bold.ttf"
));

/// Minimal Typst World for compiling markup in GORBIE.
///
/// Embeds the math font (New Computer Modern Math) and IosevkaGorbie.
/// Keeps a persistent `Source` that can be swapped for incremental
/// compilation via comemo.
pub struct GorbieWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    main_id: FileId,
    source: Source,
    fonts: Vec<Font>,
}

impl GorbieWorld {
    /// Creates a new world with embedded fonts and an empty source.
    pub fn new() -> Self {
        let mut book = FontBook::new();
        let mut fonts = Vec::new();

        // Load embedded fonts.
        for data in [MATH_FONT, BODY_FONT, BODY_FONT_BOLD] {
            let bytes = Bytes::new(data);
            for font in Font::iter(bytes) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }

        let main_id = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(main_id, String::new());

        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            main_id,
            source,
            fonts,
        }
    }

    /// Access the current source (for span resolution during copy).
    pub fn main_source(&self) -> &Source {
        &self.source
    }

    /// Set the source content for the next compilation.
    /// Uses `Source::replace` for incremental reparsing — only the
    /// changed portion of the syntax tree is rebuilt.
    pub fn set_source(&mut self, text: String) {
        self.source.replace(&text);
    }

    /// Compile the current source into a paged document.
    pub fn compile(&self) -> Result<PagedDocument, Vec<TypstDiag>> {
        let result = typst::compile::<PagedDocument>(self);
        match result.output {
            Ok(doc) => Ok(doc),
            Err(errors) => {
                let src = &self.source;
                let diags = errors
                    .iter()
                    .map(|e| TypstDiag {
                        severity: e.severity,
                        message: e.message.to_string(),
                        hints: e.hints.iter().map(|h| h.to_string()).collect(),
                        span_range: src.range(e.span),
                    })
                    .collect();
                Err(diags)
            }
        }
    }
}

impl World for GorbieWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}
