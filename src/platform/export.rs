use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::NaiveDate;
use serde::Serialize;
use thiserror::Error;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};

use crate::domain::models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput};
use crate::domain::money::format_eur;
use crate::domain::render::render_document_html;

use super::paths::{self, PathError};
use super::pdf_renderer::{PdfRenderError, render_pdf_to_pngs};
use super::png_stack::{PngStackError, stack_pages_vertically};

const TEMPLATE: &str = include_str!("../../templates/document.typ");
const LOGO: &[u8] = include_bytes!("../../templates/logo.png");
const FONT_DATA: [&[u8]; 6] = [
    include_bytes!("../../assets/fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../../assets/fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../../assets/fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/LiberationSans-Italic.ttf"),
];
const REFERENCE_NUMBER: i64 = 9;
const MAX_PUBLISHED_GENERATIONS: usize = 3;
static EXPORT_LOCK: Mutex<()> = Mutex::new(());
static GENERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct ReferenceExport {
    pub pdf_path: PathBuf,
    pub html_path: PathBuf,
    pub pages: usize,
    pub elapsed: Duration,
}

#[derive(Debug)]
pub struct DocumentExport {
    pub pdf_path: PathBuf,
    pub png_path: PathBuf,
}

/// Exports an issued document as `{devis|facture}-{n}.pdf` and `.png` under
/// `exports/` (ARCHI §4). Existing files are kept as-is (issued documents are
/// frozen); only missing files are regenerated, so the aperçu's « Exporter »
/// action doubles as the re-export path.
pub fn export_document(input: &DocumentInput, number: i64) -> Result<DocumentExport, ExportError> {
    // ponytail: one export at a time (reference or document); revisit only if
    // production needs parallel jobs.
    let _guard = EXPORT_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let exports = paths::exports_dir()?;
    fs::create_dir_all(&exports).map_err(|source| storage_error(&exports, source))?;
    export_document_in(&exports, input, number, &render_pdf_pages)
}

fn render_pdf_pages(pdf_path: &Path) -> Result<Vec<Vec<u8>>, ExportError> {
    render_pdf_to_pngs(pdf_path).map_err(ExportError::Renderer)
}

/// Injected PDF rasterizer so the export orchestration stays host-testable
/// (production: `render_pdf_to_pngs`; tests: fakes).
type PageRenderer = dyn Fn(&Path) -> Result<Vec<Vec<u8>>, ExportError>;

fn export_document_in(
    exports: &Path,
    input: &DocumentInput,
    number: i64,
    render_pages: &PageRenderer,
) -> Result<DocumentExport, ExportError> {
    let stem = export_stem(&input.kind, number);
    let pdf_path = exports.join(format!("{stem}.pdf"));
    let png_path = exports.join(format!("{stem}.png"));

    if !pdf_path.exists() {
        let pdf = compile_pdf(input, number)?;
        write_file_atomic(&pdf_path, &pdf.bytes)?;
    }
    if !png_path.exists() {
        let pages = render_pages(&pdf_path)?;
        let png = match pages.as_slice() {
            [] => return Err(PdfRenderError::NoPages.into()),
            [single] => single.clone(),
            many => stack_pages_vertically(many)?,
        };
        write_file_atomic(&png_path, &png)?;
    }

    Ok(DocumentExport { pdf_path, png_path })
}

fn export_stem(kind: &DocumentKind, number: i64) -> String {
    format!("{}-{number}", kind_label(kind))
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    let result = write_file(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path).map_err(|source| storage_error(path, source)));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("Impossible de préparer les données du document PDF.")]
    Data(#[source] serde_json::Error),
    #[error("Impossible de générer le document PDF.")]
    Typst(#[source] TypstFailure),
    #[error(transparent)]
    Renderer(#[from] PdfRenderError),
    #[error(transparent)]
    Png(#[from] PngStackError),
    #[error("Impossible d'écrire le fichier {path} dans le stockage privé.")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct TypstFailure(String);

pub fn generate_reference_export() -> Result<ReferenceExport, ExportError> {
    // ponytail: one debug export at a time; revisit only if production needs parallel jobs.
    let _guard = EXPORT_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let started = Instant::now();
    let input = reference_document();
    let pdf = compile_reference_pdf(&input)?;
    let html = render_document_html(&input, REFERENCE_NUMBER);
    let exports = paths::exports_dir()?;
    fs::create_dir_all(&exports).map_err(|source| storage_error(&exports, source))?;
    let (html_path, pdf_path) = publish_generation(&exports, html.as_bytes(), &pdf.bytes)?;

    Ok(ReferenceExport {
        pdf_path,
        html_path,
        pages: pdf.pages,
        elapsed: started.elapsed(),
    })
}

struct CompiledPdf {
    bytes: Vec<u8>,
    pages: usize,
    #[cfg(test)]
    page_texts: Vec<String>,
}

fn compile_reference_pdf(input: &DocumentInput) -> Result<CompiledPdf, ExportError> {
    compile_pdf(input, REFERENCE_NUMBER)
}

fn compile_pdf(input: &DocumentInput, number: i64) -> Result<CompiledPdf, ExportError> {
    let data = TemplateData::new(input, number);
    let json = serde_json::to_vec(&data).map_err(ExportError::Data)?;
    let world = EmbeddedWorld::new(json)?;
    let compiled = typst::compile(&world);

    for warning in &compiled.warnings {
        eprintln!("Typst warning: {}", warning.message);
    }

    let document = compiled
        .output
        .map_err(|errors| ExportError::Typst(diagnostics("compilation", &errors, &world)))?;
    let bytes = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| ExportError::Typst(diagnostics("PDF export", &errors, &world)))?;
    let pages = document.pages().len();
    #[cfg(test)]
    let page_texts = document
        .pages()
        .iter()
        .map(|page| frame_text(&page.frame))
        .collect();

    Ok(CompiledPdf {
        bytes,
        pages,
        #[cfg(test)]
        page_texts,
    })
}

#[cfg(test)]
fn frame_text(frame: &typst::layout::Frame) -> String {
    let mut output = String::new();
    for (_, item) in frame.items() {
        match item {
            typst::layout::FrameItem::Group(group) => output.push_str(&frame_text(&group.frame)),
            typst::layout::FrameItem::Text(text) => output.push_str(&text.text),
            _ => {}
        }
    }
    output
}

fn publish_generation(
    exports: &Path,
    html: &[u8],
    pdf: &[u8],
) -> Result<(PathBuf, PathBuf), ExportError> {
    let (staging, published) = create_generation_directory(exports)?;

    let result = (|| {
        write_file(&staging.join("reference.html"), html)?;
        write_file(&staging.join("candidate.pdf"), pdf)?;
        fs::rename(&staging, &published).map_err(|source| storage_error(&published, source))
    })();

    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_dir_all(&staging) {
            eprintln!("Export staging cleanup failed: {cleanup_error}");
        }
        return Err(error);
    }

    prune_published_generations(exports, &published);

    Ok((
        published.join("reference.html"),
        published.join("candidate.pdf"),
    ))
}

fn create_generation_directory(exports: &Path) -> Result<(PathBuf, PathBuf), ExportError> {
    loop {
        let sequence = GENERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // Fixed-width timestamp first keeps directory names chronologically sortable.
        let name = format!(
            "reference-{:039}-{}-{sequence:020}",
            generation_timestamp(),
            std::process::id(),
        );
        let staging = exports.join(format!(".{name}.tmp"));
        let published = exports.join(name);

        if published.exists() {
            continue;
        }

        match fs::create_dir(&staging) {
            Ok(()) => return Ok((staging, published)),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(storage_error(&staging, source)),
        }
    }
}

fn prune_published_generations(exports: &Path, current: &Path) {
    let entries = match fs::read_dir(exports) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("Export retention scan failed: {error}");
            return;
        }
    };

    let mut previous = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if !entry.file_type().ok()?.is_dir() || path == current {
                return None;
            }
            let order = generation_order(entry.file_name().to_str()?)?;
            Some((order, path))
        })
        .collect::<Vec<_>>();
    previous.sort();

    let excess = (previous.len() + 1).saturating_sub(MAX_PUBLISHED_GENERATIONS);
    for (_, path) in previous.into_iter().take(excess) {
        if let Err(error) = fs::remove_dir_all(&path) {
            eprintln!(
                "Export retention cleanup failed for {}: {error}",
                path.display()
            );
        }
    }
}

fn generation_order(name: &str) -> Option<(u128, u64)> {
    let mut parts = name.strip_prefix("reference-")?.split('-');
    let first = parts.next()?;
    let second = parts.next()?;
    match (parts.next(), parts.next()) {
        // Current format: timestamp-pid-sequence.
        (Some(sequence), None) => {
            second.parse::<u32>().ok()?;
            Some((first.parse().ok()?, sequence.parse().ok()?))
        }
        // Compatibility with the earlier pid-timestamp format.
        (None, None) => {
            first.parse::<u32>().ok()?;
            Some((second.parse().ok()?, 0))
        }
        _ => None,
    }
}

fn generation_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    fs::write(path, bytes).map_err(|source| storage_error(path, source))
}

fn storage_error(path: &Path, source: std::io::Error) -> ExportError {
    ExportError::Write {
        path: path.display().to_string(),
        source,
    }
}

fn diagnostics(stage: &str, errors: &[SourceDiagnostic], world: &EmbeddedWorld) -> TypstFailure {
    let messages = errors
        .iter()
        .map(|error| {
            let position = world
                .range(error.span)
                .and_then(|range| world.source.lines().byte_to_line_column(range.start))
                .map(|(line, column)| format!("{}:{}", line + 1, column + 1))
                .unwrap_or_else(|| "unknown location".to_string());
            format!("{position}: {}", error.message)
        })
        .collect::<Vec<_>>()
        .join("; ");
    TypstFailure(format!("Typst {stage} failed: {messages}"))
}

struct EmbeddedWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    json_id: FileId,
    logo_id: FileId,
    source: Source,
    source_bytes: Bytes,
    json: Bytes,
    logo: Bytes,
}

impl EmbeddedWorld {
    fn new(json: Vec<u8>) -> Result<Self, ExportError> {
        let main_id = embedded_id("document.typ")?;
        let json_id = embedded_id("document.json")?;
        let logo_id = embedded_id("logo.png")?;
        let fonts = FONT_DATA
            .into_iter()
            .filter_map(|data| Font::new(Bytes::new(data), 0))
            .collect::<Vec<_>>();
        if fonts.len() != FONT_DATA.len() {
            return Err(ExportError::Typst(TypstFailure(
                "one or more embedded Liberation fonts are invalid".to_string(),
            )));
        }

        let source = Source::new(main_id, TEMPLATE.to_string());
        Ok(Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            main_id,
            json_id,
            logo_id,
            source,
            source_bytes: Bytes::from_string(TEMPLATE),
            json: Bytes::new(json),
            logo: Bytes::new(LOGO),
        })
    }
}

impl World for EmbeddedWorld {
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
            Err(FileError::NotSource)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main_id {
            Ok(self.source_bytes.clone())
        } else if id == self.json_id {
            Ok(self.json.clone())
        } else if id == self.logo_id {
            Ok(self.logo.clone())
        } else {
            Err(FileError::Other(Some("embedded asset unavailable".into())))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

fn embedded_id(path: &str) -> Result<FileId, ExportError> {
    let path = VirtualPath::new(path).map_err(|error| {
        ExportError::Typst(TypstFailure(format!("invalid embedded path: {error}")))
    })?;
    Ok(RootedPath::new(VirtualRoot::Project, path).intern())
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TemplateData {
    quote: bool,
    title: &'static str,
    nature: &'static str,
    number_label: &'static str,
    number: i64,
    issue_date: String,
    event_date: String,
    validity_end: String,
    payment_terms: String,
    total: String,
    total_label: String,
    professional: bool,
    client: TemplateClient,
    groups: Vec<TemplateGroup>,
}

/// French filename label for the document kind (« devis »/« facture »).
fn kind_label(kind: &DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Quote => "devis",
        DocumentKind::Invoice => "facture",
    }
}

impl TemplateData {
    fn new(input: &DocumentInput, number: i64) -> Self {
        let is_quote = matches!(input.kind, DocumentKind::Quote);
        let event_date = format_date(&input.event_date);
        Self {
            quote: is_quote,
            title: if is_quote { "DEVIS" } else { "FACTURE" },
            nature: if is_quote {
                "Offre gratuite et sans engagement"
            } else {
                "Merci de votre confiance"
            },
            number_label: if is_quote {
                "N° de devis"
            } else {
                "N° de facture"
            },
            number,
            issue_date: format_date(&input.issue_date),
            validity_end: validity_end(&input.issue_date),
            payment_terms: input.payment_terms.clone(),
            total: format_eur(input.total_cents()),
            total_label: if is_quote {
                "Total du devis".to_string()
            } else {
                format!("Total net à payer avant le {event_date}")
            },
            professional: matches!(input.client.kind, ClientKind::Professional),
            client: TemplateClient::from(&input.client),
            groups: template_groups(input),
            event_date,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TemplateClient {
    name: String,
    address: String,
    email: String,
    phone: String,
    business_id: String,
    billing_address: String,
}

impl From<&ClientInput> for TemplateClient {
    fn from(client: &ClientInput) -> Self {
        Self {
            name: client.name.clone(),
            address: client.address.clone(),
            email: client.email.clone().unwrap_or_default(),
            phone: client.phone.clone().unwrap_or_default(),
            business_id: client.business_id.clone().unwrap_or_default(),
            billing_address: client.billing_address.clone().unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
struct TemplateGroup {
    name: String,
    lines: Vec<TemplateLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TemplateLine {
    description: String,
    quantity: i64,
    unit_price: String,
    amount: String,
    alternate: bool,
}

fn template_groups(input: &DocumentInput) -> Vec<TemplateGroup> {
    let mut groups: Vec<(String, Vec<&LineInput>)> = Vec::new();
    for line in &input.lines {
        let name = line
            .group
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("Prestations");
        if let Some((_, lines)) = groups.iter_mut().find(|(existing, _)| existing == name) {
            lines.push(line);
        } else {
            groups.push((name.to_string(), vec![line]));
        }
    }

    let mut index = 0_usize;
    groups
        .into_iter()
        .map(|(name, lines)| TemplateGroup {
            name,
            lines: lines
                .into_iter()
                .map(|line| {
                    let template = TemplateLine {
                        description: line.description.clone(),
                        quantity: line.quantity,
                        unit_price: format_eur(line.unit_price_cents),
                        amount: format_eur(line.amount_cents()),
                        alternate: index % 2 == 1,
                    };
                    index += 1;
                    template
                })
                .collect(),
        })
        .collect()
}

fn format_date(value: &str) -> String {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn validity_end(issue_date: &str) -> String {
    NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.checked_add_days(chrono::Days::new(30)))
        .map(|date| date.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|| "30 jours".to_string())
}

fn reference_document() -> DocumentInput {
    let groups = ["Cocktail salé", "Buffet chaud", "Desserts", "Boissons"];
    let lines = (0..48)
        .map(|index| LineInput {
            group: Some(groups[index % groups.len()].to_string()),
            description: format!(
                "Prestation de référence {:02} avec une désignation assez longue",
                index + 1
            ),
            quantity: 25 + index as i64,
            unit_price_cents: 175,
        })
        .collect();

    DocumentInput {
        kind: DocumentKind::Quote,
        issue_date: "2026-07-21".to_string(),
        event_date: "2026-08-15".to_string(),
        payment_terms: "à réception".to_string(),
        client: ClientInput {
            kind: ClientKind::Professional,
            name: "Association Les Gourmets de Charente".to_string(),
            address: "12 avenue des Tilleuls, 17000 La Rochelle".to_string(),
            email: Some("contact@gourmets.example".to_string()),
            phone: Some("05 46 00 00 00".to_string()),
            business_id: Some("SIRET 123 456 789 00012".to_string()),
            billing_address: Some(
                "Service comptabilité, 4 place du Marché, 17000 La Rochelle".to_string(),
            ),
        },
        lines,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        ExportError, MAX_PUBLISHED_GENERATIONS, PdfRenderError, compile_pdf, compile_reference_pdf,
        export_document_in, generation_order, generation_timestamp, publish_generation,
        reference_document,
    };
    use crate::domain::models::{ClientKind, DocumentKind};
    use crate::platform::png_stack::PAGE_SEPARATOR_HEIGHT;

    #[test]
    fn compiles_reference_document_to_multi_page_pdf() {
        let input = reference_document();

        let pdf = compile_reference_pdf(&input).expect("reference PDF should compile");

        assert!(pdf.bytes.starts_with(b"%PDF-"));
        assert!(pdf.pages > 1, "expected multiple pages, got {}", pdf.pages);
    }

    #[test]
    fn compiles_an_invoice_with_the_payment_block() {
        let mut input = reference_document();
        input.kind = DocumentKind::Invoice;
        input.payment_terms = "Comptant".to_string();

        let pdf = compile_pdf(&input, 3).expect("invoice PDF should compile");

        let text = pdf.page_texts.join(" ");
        assert!(text.contains("FACTURE"), "invoice title missing: {text}");
        assert!(text.contains("N° de facture"));
        assert!(text.contains("Règlement"));
        assert!(text.contains("Comptant"));
        assert!(text.contains("Total net à payer"));
        // Professional client: the late-payment penalties are mandatory.
        assert!(text.contains("Pénalités de retard"));
        assert!(
            !text.contains("Offre gratuite"),
            "quote subtitle must not appear on an invoice"
        );
        assert!(!text.contains("Bon pour accord"));
    }

    #[test]
    fn paginates_a_group_larger_than_one_page() {
        let mut input = reference_document();
        input.lines.extend(input.lines.clone());
        for line in &mut input.lines {
            line.group = Some("Groupe long".to_string());
        }

        let pdf = compile_reference_pdf(&input).expect("long group should compile");

        assert!(
            pdf.pages > 2,
            "expected at least 3 pages, got {}",
            pdf.pages
        );
        assert!(
            pdf.page_texts
                .iter()
                .all(|page| page.contains("Groupe long")),
            "the group name should repeat on every continuation page"
        );
    }

    #[test]
    fn publishes_artifacts_in_the_same_generation() {
        let exports = std::env::temp_dir().join(format!(
            "devis-mobile-export-test-{}-{}",
            std::process::id(),
            generation_timestamp()
        ));
        fs::create_dir(&exports).expect("test export directory should be created");

        let (html, pdf) = publish_generation(&exports, b"html", b"pdf")
            .expect("artifacts should be published together");

        assert_eq!(html.parent(), pdf.parent());
        assert_eq!(fs::read(html).expect("HTML should exist"), b"html");
        assert_eq!(fs::read(pdf).expect("PDF should exist"), b"pdf");
        fs::remove_dir_all(exports).expect("test export directory should be removed");
    }

    #[test]
    fn gives_each_generation_a_unique_directory() {
        let exports = std::env::temp_dir().join(format!(
            "devis-mobile-export-unique-test-{}-{}",
            std::process::id(),
            generation_timestamp()
        ));
        fs::create_dir(&exports).expect("test export directory should be created");

        let first = publish_generation(&exports, b"first html", b"first pdf")
            .expect("first generation should be published");
        let second = publish_generation(&exports, b"second html", b"second pdf")
            .expect("second generation should be published");

        assert_ne!(first.0.parent(), second.0.parent());
        assert!(first.0.exists());
        assert!(second.0.exists());
        fs::remove_dir_all(exports).expect("test export directory should be removed");
    }

    #[test]
    fn retains_only_the_latest_generations() {
        let exports = std::env::temp_dir().join(format!(
            "devis-mobile-export-retention-test-{}-{}",
            std::process::id(),
            generation_timestamp()
        ));
        fs::create_dir(&exports).expect("test export directory should be created");

        let mut latest_html = None;
        for index in 0..(MAX_PUBLISHED_GENERATIONS + 2) {
            let html = format!("html {index}");
            let (html_path, _) = publish_generation(&exports, html.as_bytes(), b"pdf")
                .expect("generation should be published");
            latest_html = Some(html_path);
        }

        let generation_count = fs::read_dir(&exports)
            .expect("export directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count();
        assert_eq!(generation_count, MAX_PUBLISHED_GENERATIONS);
        assert!(
            latest_html
                .expect("latest generation should exist")
                .exists()
        );
        fs::remove_dir_all(exports).expect("test export directory should be removed");
    }

    #[test]
    fn derives_retention_order_from_current_and_legacy_names() {
        assert_eq!(
            generation_order(
                "reference-000000000000000000000000000000000000123-42-00000000000000000007"
            ),
            Some((123, 7))
        );
        assert_eq!(generation_order("reference-42-122"), Some((122, 0)));
        assert_eq!(generation_order("reference-unrelated"), None);
    }

    fn fake_page_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba(color),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode fake page");
        bytes.into_inner()
    }

    fn fake_pages(count: usize) -> impl Fn(&std::path::Path) -> Result<Vec<Vec<u8>>, ExportError> {
        move |_pdf| {
            Ok((0..count)
                .map(|index| fake_page_png(10, 4 + index as u32, [index as u8, 0, 0, 255]))
                .collect())
        }
    }

    fn temp_exports(tag: &str) -> std::path::PathBuf {
        let exports = std::env::temp_dir().join(format!(
            "devis-mobile-export-document-{tag}-{}-{}",
            std::process::id(),
            generation_timestamp()
        ));
        fs::create_dir(&exports).expect("test export directory should be created");
        exports
    }

    #[test]
    fn exports_pdf_and_stacked_png_with_the_french_stem() {
        let exports = temp_exports("full");
        let input = reference_document();

        let export = export_document_in(&exports, &input, 10, &fake_pages(2))
            .expect("document export should succeed");

        assert_eq!(
            export.pdf_path.file_name().and_then(|n| n.to_str()),
            Some("devis-10.pdf")
        );
        assert_eq!(
            export.png_path.file_name().and_then(|n| n.to_str()),
            Some("devis-10.png")
        );
        assert!(
            fs::read(&export.pdf_path)
                .expect("pdf")
                .starts_with(b"%PDF-")
        );
        let png = image::load_from_memory(&fs::read(&export.png_path).expect("png"))
            .expect("decode png")
            .into_rgba8();
        // Two fake pages (4px and 5px tall) plus the separator.
        assert_eq!(png.height(), 4 + PAGE_SEPARATOR_HEIGHT + 5);
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn names_invoice_exports_with_the_facture_stem() {
        let exports = temp_exports("stem");
        let mut input = reference_document();
        input.kind = DocumentKind::Invoice;
        input.payment_terms = "Comptant".to_string();

        let export = export_document_in(&exports, &input, 3, &fake_pages(1))
            .expect("document export should succeed");

        assert_eq!(
            export.pdf_path.file_name().and_then(|n| n.to_str()),
            Some("facture-3.pdf")
        );
        assert_eq!(
            export.png_path.file_name().and_then(|n| n.to_str()),
            Some("facture-3.png")
        );
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn keeps_existing_files_on_reexport() {
        let exports = temp_exports("keep");
        let input = reference_document();
        fs::write(exports.join("devis-10.pdf"), b"original pdf").expect("seed pdf");
        fs::write(exports.join("devis-10.png"), b"original png").expect("seed png");
        let forbidden_renderer = |_pdf: &std::path::Path| -> Result<Vec<Vec<u8>>, ExportError> {
            panic!("the renderer must not run when both files exist")
        };

        let export = export_document_in(&exports, &input, 10, &forbidden_renderer)
            .expect("reexport should succeed");

        assert_eq!(fs::read(&export.pdf_path).expect("pdf"), b"original pdf");
        assert_eq!(fs::read(&export.png_path).expect("png"), b"original png");
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn regenerates_only_the_missing_png() {
        let exports = temp_exports("missing");
        let input = reference_document();
        fs::write(exports.join("devis-10.pdf"), b"original pdf").expect("seed pdf");

        let export = export_document_in(&exports, &input, 10, &fake_pages(1))
            .expect("reexport should succeed");

        assert_eq!(fs::read(&export.pdf_path).expect("pdf"), b"original pdf");
        let png = image::load_from_memory(&fs::read(&export.png_path).expect("png"))
            .expect("decode png")
            .into_rgba8();
        assert_eq!(png.height(), 4);
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn renderer_failure_surfaces_an_error_and_keeps_the_pdf() {
        let exports = temp_exports("failure");
        let input = reference_document();
        let failing_renderer = |_pdf: &std::path::Path| -> Result<Vec<Vec<u8>>, ExportError> {
            Err(PdfRenderError::Unsupported.into())
        };

        let result = export_document_in(&exports, &input, 10, &failing_renderer);

        assert!(result.is_err());
        assert!(
            exports.join("devis-10.pdf").exists(),
            "the PDF must be kept"
        );
        assert!(!exports.join("devis-10.png").exists());
        assert!(!exports.join("devis-10.png.tmp").exists());
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn zero_rendered_pages_is_an_error() {
        let exports = temp_exports("nopages");
        let input = reference_document();

        let result = export_document_in(&exports, &input, 10, &fake_pages(0));

        assert!(result.is_err());
        assert!(!exports.join("devis-10.png").exists());
        fs::remove_dir_all(exports).expect("cleanup");
    }

    #[test]
    fn invoice_for_an_individual_omits_the_late_penalties() {
        let mut input = reference_document();
        input.kind = DocumentKind::Invoice;
        input.payment_terms = "Comptant".to_string();
        input.client.kind = ClientKind::Individual;

        let pdf = compile_pdf(&input, 4).expect("invoice PDF should compile");

        let text = pdf.page_texts.join(" ");
        assert!(text.contains("FACTURE"));
        assert!(!text.contains("Pénalités de retard"));
    }

    #[test]
    fn quote_keeps_its_conditions_and_signature_blocks() {
        let input = reference_document();

        let pdf = compile_pdf(&input, 9).expect("quote PDF should compile");

        let text = pdf.page_texts.join(" ");
        assert!(text.contains("DEVIS"));
        assert!(text.contains("N° de devis"));
        assert!(text.contains("Total du devis"));
        assert!(text.contains("Bon pour accord"));
        assert!(!text.contains("FACTURE"));
    }
}
