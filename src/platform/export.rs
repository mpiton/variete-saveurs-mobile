use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

#[derive(Debug)]
pub struct ReferenceExport {
    pub pdf_path: PathBuf,
    pub html_path: PathBuf,
    pub pages: usize,
    pub elapsed: Duration,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("Impossible de préparer les données du document PDF.")]
    Data(#[source] serde_json::Error),
    #[error("Impossible de générer le document PDF.")]
    Typst(#[source] TypstFailure),
    #[error("Impossible d'écrire le document PDF dans le stockage privé.")]
    Write(#[source] std::io::Error),
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct TypstFailure(String);

pub fn generate_reference_export() -> Result<ReferenceExport, ExportError> {
    let started = Instant::now();
    let input = reference_document();
    let pdf = compile_reference_pdf(&input)?;
    let html = render_document_html(&input, REFERENCE_NUMBER);
    let exports = paths::exports_dir()?;
    fs::create_dir_all(&exports).map_err(ExportError::Write)?;

    let html_path = exports.join("reference.html");
    let pdf_path = exports.join("candidate.pdf");
    write_atomically(&html_path, html.as_bytes())?;
    write_atomically(&pdf_path, &pdf.bytes)?;

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
}

fn compile_reference_pdf(input: &DocumentInput) -> Result<CompiledPdf, ExportError> {
    let data = TemplateData::new(input);
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

    Ok(CompiledPdf { bytes, pages })
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    let temporary = path.with_extension(format!("{extension}.tmp"));
    fs::write(&temporary, bytes).map_err(ExportError::Write)?;
    fs::rename(temporary, path).map_err(ExportError::Write)
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
    number: i64,
    issue_date: String,
    event_date: String,
    validity_end: String,
    total: String,
    client: TemplateClient,
    groups: Vec<TemplateGroup>,
}

impl TemplateData {
    fn new(input: &DocumentInput) -> Self {
        Self {
            number: REFERENCE_NUMBER,
            issue_date: format_date(&input.issue_date),
            event_date: format_date(&input.event_date),
            validity_end: validity_end(&input.issue_date),
            total: format_eur(input.total_cents()),
            client: TemplateClient::from(&input.client),
            groups: template_groups(input),
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
    use super::{compile_reference_pdf, reference_document};

    #[test]
    fn compiles_reference_document_to_multi_page_pdf() {
        let input = reference_document();

        let pdf = compile_reference_pdf(&input).expect("reference PDF should compile");

        assert!(pdf.bytes.starts_with(b"%PDF-"));
        assert!(pdf.pages > 1, "expected multiple pages, got {}", pdf.pages);
    }
}
