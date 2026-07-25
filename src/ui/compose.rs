//! Compose screen (DESIGN.md §5, ARCHI.md §4 « Envoi email », task 27):
//! recipient pre-filled from the client, subject and body pre-filled from the
//! branded template (all freely retouchable, never persisted — interview
//! decision), PDF ○ PNG attachment radio (PDF default) and a single
//! « Envoyer » with a loading state. One call, one verdict: success marks the
//! document sent (`mark_sent`, first send only) and returns to the fiche with
//! a snackbar; failure stays on screen as a persistent French block — no
//! queue, no retry of its own.

use std::panic::{AssertUnwindSafe, catch_unwind};

use chrono::Utc;
use dioxus::prelude::*;
use rusqlite::Connection;

use crate::{
    domain::{
        db::{get_document, mark_sent},
        email::{Attachment, EmailPlaceholders, MailConfig, Mailer, render_email},
        models::{Document, DocumentKind},
        render::validity_end_date,
        settings::load_email_credentials,
        validation::plausible_email,
    },
    platform::{
        export::{DocumentExport, export_document},
        mail::BrevoMailer,
    },
};

use super::{
    app::DatabaseContext,
    components::{Button, ErrorBlock, OutlinedField, OutlinedTextArea, ShareFormat},
    issue::write_from_worker,
    share::share_file_names,
};

/// Success notice published by the compose screen right before it navigates
/// back: the fiche (which remounts and reloads, so the « envoyé » badge is
/// already there) displays and clears it. Provided once at the app root.
#[derive(Clone, Copy)]
pub(super) struct SendNotice(pub Signal<Option<String>>);

/// State of a send job, driven from a worker thread (`SyncStorage`).
#[derive(Debug, Clone, Default, PartialEq)]
enum SendPhase {
    #[default]
    Idle,
    Running,
    /// Terminal: the UI effect publishes the notice and navigates back.
    Done,
    /// Persistent block on the screen (DESIGN.md §6), cleared by the next try.
    Failed(String),
}

/// What the compose screen shows on entry, loaded fresh from the local
/// database: the document (for the export and the send) plus the pre-filled
/// recipient, subject and body. After that the fields are working copies —
/// retouches are never written back.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ComposeData {
    pub document: Document,
    pub to: String,
    pub subject: String,
    pub body_html: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComposeError {
    DocumentNotFound,
    Unavailable,
}

pub(super) fn load_compose(connection: &Connection, id: i64) -> Result<ComposeData, ComposeError> {
    let document = get_document(connection, id).map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => ComposeError::DocumentNotFound,
        error => {
            eprintln!("Compose document query failed: {error}");
            ComposeError::Unavailable
        }
    })?;
    let input = &document.input;
    // The validity sentence matches the document itself (« valable jusqu'au »
    // = issue date + 30 days, rule owned by `render`), so the email never
    // promises a date the PDF contradicts.
    let content = render_email(
        &input.kind,
        &EmailPlaceholders {
            doc_number: document.number,
            client_name: input.client.name.clone(),
            total_cents: document.total_cents,
            validity_date: match input.kind {
                DocumentKind::Quote => validity_end_date(&input.issue_date),
                DocumentKind::Invoice => None,
            },
        },
    );
    Ok(ComposeData {
        to: input.client.email.clone().unwrap_or_default(),
        subject: content.subject,
        body_html: content.body_html,
        document,
    })
}

/// Simple recipient gate (task 27): an empty or implausible address blocks
/// the send with a French explanation — a wrong-but-plausible one surfaces at
/// send time via Brevo instead.
pub(super) fn recipient_error(to: &str) -> Option<String> {
    let trimmed = to.trim();
    if trimmed.is_empty() {
        return Some("Saisissez l’adresse email du destinataire.".to_string());
    }
    if !plausible_email(trimmed) {
        return Some("Cette adresse email semble incorrecte.".to_string());
    }
    None
}

/// Reads the picked export as the attachment (« devis-10.pdf »). The export
/// step just ran, so a read failure is a genuine storage problem.
fn attachment_from_export(
    export: &DocumentExport,
    kind: &DocumentKind,
    number: i64,
    format: ShareFormat,
) -> Result<Attachment, String> {
    let (pdf_name, png_name) = share_file_names(kind, number);
    let (path, name) = match format {
        ShareFormat::Pdf => (&export.pdf_path, pdf_name),
        ShareFormat::Png => (&export.png_path, png_name),
    };
    let bytes = std::fs::read(path).map_err(|error| {
        eprintln!(
            "Compose attachment read failed for {}: {error}",
            path.display()
        );
        "Impossible de lire le fichier à joindre. Réessayez l’envoi.".to_string()
    })?;
    Ok(Attachment { name, bytes })
}

/// Sending configuration for one attempt. Reaching the compose screen means
/// the fiche's gate was open, but settings can have been cleared since —
/// `None` becomes the same French explanation as the fiche's hint.
pub(super) fn load_send_config(connection: &Connection) -> Result<MailConfig, String> {
    load_email_credentials(connection)
        .map_err(|error| {
            eprintln!("Compose settings query failed: {error}");
            "Impossible de lire la configuration d’envoi.".to_string()
        })?
        .ok_or_else(|| {
            "Envoi indisponible : configurez la clé Brevo et l’expéditeur dans Réglages."
                .to_string()
        })
}

/// Marks the document sent after a SUCCESSFUL send only. `mark_sent` keeps
/// the first `sent_at` on resends (task 08) — a false return is the normal
/// resend path, not an error. A write failure here means the email IS gone:
/// the message must not read like a send failure.
pub(super) fn mark_sent_after_send(
    connection: &Connection,
    document_id: i64,
    now: &str,
) -> Result<(), String> {
    mark_sent(connection, document_id, now)
        .map(|_| ())
        .map_err(|error| {
            eprintln!("mark_sent failed after a successful send: {error}");
            "L’email est parti, mais le statut « envoyé » n’a pas pu être enregistré.".to_string()
        })
}

/// Worker body: export (generates the picked file if missing, ARCHI §4),
/// build the attachment, send, then mark. The database lock never wraps the
/// network call — credentials are read under a short lock, `mark_sent` runs
/// under a fresh one, so a slow Brevo answer can't freeze another screen.
fn run_send(
    database: &DatabaseContext,
    document: &Document,
    to: &str,
    subject: &str,
    body_html: &str,
    format: ShareFormat,
) -> Result<(), String> {
    let export = export_document(&document.input, document.number).map_err(|error| {
        eprintln!("Compose export failed for {}: {error}", document.number);
        error.to_string()
    })?;
    let attachment =
        attachment_from_export(&export, &document.input.kind, document.number, format)?;
    let config = {
        let connection = lock_database(database)?;
        load_send_config(&connection)?
    };
    BrevoMailer
        .send_document_email(&config, to, subject, body_html, &attachment)
        .map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    let connection = lock_database(database)?;
    mark_sent_after_send(&connection, document.id, &now)
}

fn lock_database(
    database: &DatabaseContext,
) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())
}

/// Starts the send job. The phase guards the double-tap: a second call while
/// `Running` returns immediately (the button is also inert while loading).
fn start_send(
    mut state: Signal<SendPhase, SyncStorage>,
    database: DatabaseContext,
    document: Document,
    to: String,
    subject: String,
    body_html: String,
    format: ShareFormat,
) {
    if matches!(&*state.read(), SendPhase::Running) {
        return;
    }
    *state.write() = SendPhase::Running;
    let worker = std::thread::Builder::new().spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            run_send(&database, &document, &to, &subject, &body_html, format)
        }));
        let next = match outcome {
            Ok(Ok(())) => SendPhase::Done,
            Ok(Err(message)) => SendPhase::Failed(message),
            Err(payload) => {
                eprintln!("Send panicked: {payload:?}");
                SendPhase::Failed("Échec inattendu de l’envoi (détail dans les logs).".to_string())
            }
        };
        write_from_worker(state, |phase| *phase = next);
    });
    // Fallible spawn: a resource-starved OS must not panic the UI thread —
    // the job goes straight to its terminal failure state.
    if let Err(error) = worker {
        eprintln!("Send worker could not start: {error}");
        write_from_worker(state, |phase| {
            *phase = SendPhase::Failed("Impossible de démarrer l’envoi.".to_string());
        });
    }
}

#[component]
pub(super) fn Compose(id: i64) -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let mut send_notice = use_context::<SendNotice>();
    // Loaded once on entry: the fields are working copies afterwards, so
    // re-renders from typing never clobber retouches with a fresh query.
    let load_database = database.clone();
    let initial = use_hook(move || load_compose_from_context(&load_database, id));
    let initial_data = initial.as_ref().ok();
    let load_error = initial.as_ref().err().copied();

    let mut to = use_signal(|| initial_data.map(|data| data.to.clone()).unwrap_or_default());
    let mut subject = use_signal(|| {
        initial_data
            .map(|data| data.subject.clone())
            .unwrap_or_default()
    });
    let mut body = use_signal(|| {
        initial_data
            .map(|data| data.body_html.clone())
            .unwrap_or_default()
    });
    let mut format = use_signal(|| ShareFormat::Pdf);
    let mut to_error = use_signal(|| None::<String>);
    let send_state = use_signal_sync(SendPhase::default);

    // Terminal success: hand the notice to the fiche and go back — the fiche
    // reloads from the database, so the « envoyé » badge is already there.
    use_effect(move || {
        if matches!(&*send_state.read(), SendPhase::Done) {
            send_notice.0.set(Some("Email envoyé".to_string()));
            navigator.go_back();
        }
    });

    if let Some(error) = load_error {
        let (title, message) = match error {
            ComposeError::DocumentNotFound => (
                "Document introuvable",
                "Ce document n’existe pas ou plus dans l’historique.",
            ),
            ComposeError::Unavailable => (
                "Chargement impossible",
                "Impossible de charger le document.",
            ),
        };
        return rsx! {
            section { class: "screen compose-screen",
                div { class: "placeholder-panel",
                    ErrorBlock {
                        title: title.to_string(),
                        message: message.to_string(),
                    }
                    Button {
                        label: "Retour".to_string(),
                        onclick: move |_| navigator.go_back(),
                    }
                }
            }
        };
    }

    let Some(data) = initial_data else {
        return rsx! {};
    };
    let document = &data.document;
    let (pdf_name, png_name) = share_file_names(&document.input.kind, document.number);
    let running = matches!(&*send_state.read(), SendPhase::Running);
    let send_error = match &*send_state.read() {
        SendPhase::Failed(message) => Some(message.clone()),
        _ => None,
    };
    let send_database = database.clone();
    let send_document = document.clone();

    rsx! {
        section { class: "screen compose-screen", aria_label: "Composition de l’envoi",
            p {
                "L’email part de votre adresse professionnelle, avec le document en pièce jointe et une copie pour vous."
            }
            OutlinedField {
                label: "Destinataire".to_string(),
                name: "compose-to".to_string(),
                input_type: "email".to_string(),
                input_mode: "email".to_string(),
                placeholder: "client@exemple.fr".to_string(),
                value: to(),
                disabled: running,
                error: to_error(),
                oninput: move |event: FormEvent| {
                    to.set(event.value());
                    to_error.set(None);
                },
            }
            OutlinedField {
                label: "Objet".to_string(),
                name: "compose-subject".to_string(),
                value: subject(),
                disabled: running,
                oninput: move |event: FormEvent| subject.set(event.value()),
            }
            OutlinedTextArea {
                label: "Message".to_string(),
                name: "compose-body".to_string(),
                value: body(),
                disabled: running,
                oninput: move |event: FormEvent| body.set(event.value()),
            }
            fieldset { class: "compose-format",
                legend { "Pièce jointe" }
                label { class: "compose-format__option",
                    input {
                        r#type: "radio",
                        name: "compose-format",
                        checked: format() == ShareFormat::Pdf,
                        disabled: running,
                        onchange: move |_| format.set(ShareFormat::Pdf),
                    }
                    "{pdf_name}"
                }
                label { class: "compose-format__option",
                    input {
                        r#type: "radio",
                        name: "compose-format",
                        checked: format() == ShareFormat::Png,
                        disabled: running,
                        onchange: move |_| format.set(ShareFormat::Png),
                    }
                    "{png_name}"
                }
            }
            if let Some(message) = send_error {
                ErrorBlock {
                    title: "Envoi impossible".to_string(),
                    message,
                }
            }
            div { class: "compose-sticky",
                footer { class: "chrome-action-bar compose-action-bar", aria_label: "Envoi",
                    Button {
                        label: "Envoyer".to_string(),
                        loading: running,
                        onclick: move |_| {
                            // The recipient gate runs on the UI thread: an
                            // invalid address never spawns a worker.
                            let recipient = to.peek().trim().to_string();
                            if let Some(message) = recipient_error(&recipient) {
                                to_error.set(Some(message));
                                return;
                            }
                            start_send(
                                send_state,
                                send_database.clone(),
                                send_document.clone(),
                                recipient,
                                subject.peek().clone(),
                                body.peek().clone(),
                                format(),
                            );
                        },
                    }
                }
            }
        }
    }
}

fn load_compose_from_context(
    database: &DatabaseContext,
    id: i64,
) -> Result<ComposeData, ComposeError> {
    let database = database.as_ref().map_err(|_| ComposeError::Unavailable)?;
    let connection = database.lock().map_err(|_| ComposeError::Unavailable)?;
    load_compose(&connection, id)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use rusqlite::Connection;
    use tempfile::{NamedTempFile, TempDir};

    use crate::domain::{
        db::{get_document, issue_document, open_database},
        email::{MailConfig, Mailer},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
        settings::save_email_settings,
    };

    use super::{
        ComposeError, ShareFormat, attachment_from_export, load_compose, load_send_config,
        mark_sent_after_send, recipient_error,
    };

    fn temp_connection() -> (NamedTempFile, Mutex<Connection>) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, connection)
    }

    fn lock(database: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
        database.lock().expect("lock database")
    }

    fn sample_input(kind: DocumentKind) -> DocumentInput {
        DocumentInput {
            kind: kind.clone(),
            issue_date: "2026-07-24".to_string(),
            event_date: "2026-08-02".to_string(),
            payment_terms: match kind {
                DocumentKind::Quote => String::new(),
                DocumentKind::Invoice => "Comptant".to_string(),
            },
            client: ClientInput {
                kind: ClientKind::Individual,
                name: "Marie Dupont".to_string(),
                address: "12 rue des Lilas, 17130 Montendre".to_string(),
                email: Some("marie@example.fr".to_string()),
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: vec![LineInput {
                group: None,
                description: "Pains spéciaux".to_string(),
                quantity: 10,
                unit_price_cents: 350,
            }],
            source_quote_id: None,
        }
    }

    #[test]
    fn the_compose_screen_prefills_recipient_subject_and_body() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let quote = issue_document(
            &mut write,
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        let data = load_compose(&write, quote.id).expect("compose data");

        assert_eq!(data.to, "marie@example.fr");
        assert_eq!(data.subject, "Devis n° 10 — Variété de Saveurs");
        assert!(data.body_html.contains("Marie Dupont"));
        assert!(data.body_html.contains("devis n° 10"));
        // The validity sentence matches the document: issue date + 30 days.
        assert!(data.body_html.contains("valable jusqu'au 23/08/2026"));
    }

    #[test]
    fn a_client_without_email_leaves_the_recipient_empty() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let mut input = sample_input(DocumentKind::Quote);
        input.client.email = None;
        let quote = issue_document(&mut write, input, "2026-07-24T10:00:00Z").expect("issue quote");

        let data = load_compose(&write, quote.id).expect("compose data");

        assert_eq!(data.to, "");
        assert!(recipient_error(&data.to).is_some());
    }

    #[test]
    fn an_invoice_body_has_no_validity_sentence() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let invoice = issue_document(
            &mut write,
            sample_input(DocumentKind::Invoice),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue invoice");

        let data = load_compose(&write, invoice.id).expect("compose data");

        assert_eq!(
            data.subject,
            format!("Facture n° {} — Variété de Saveurs", invoice.number)
        );
        assert!(!data.body_html.contains("valable jusqu'au"));
    }

    #[test]
    fn an_unknown_document_is_an_error() {
        let (_file, database) = temp_connection();
        let connection = lock(&database);
        assert_eq!(
            load_compose(&connection, 999),
            Err(ComposeError::DocumentNotFound)
        );
    }

    #[test]
    fn the_recipient_gate_rejects_empty_and_implausible_addresses() {
        assert!(recipient_error("").is_some());
        assert!(recipient_error("   ").is_some());
        assert!(recipient_error("marie").is_some());
        assert!(recipient_error("marie@").is_some());
        assert!(recipient_error("marie@exemple").is_some());
        assert_eq!(recipient_error("marie@example.fr"), None);
    }

    #[test]
    fn the_attachment_reads_the_picked_export_with_its_display_name() {
        let directory = TempDir::new().expect("temp exports dir");
        let pdf_path = directory.path().join("devis-10.pdf");
        let png_path = directory.path().join("devis-10.png");
        std::fs::write(&pdf_path, b"pdf-bytes").expect("write pdf");
        std::fs::write(&png_path, b"png-bytes").expect("write png");
        let export = crate::platform::export::DocumentExport { pdf_path, png_path };

        let pdf = attachment_from_export(&export, &DocumentKind::Quote, 10, ShareFormat::Pdf)
            .expect("pdf attachment");
        assert_eq!(pdf.name, "devis-10.pdf");
        assert_eq!(pdf.bytes, b"pdf-bytes");

        let png = attachment_from_export(&export, &DocumentKind::Quote, 10, ShareFormat::Png)
            .expect("png attachment");
        assert_eq!(png.name, "devis-10.png");
        assert_eq!(png.bytes, b"png-bytes");
    }

    #[test]
    fn a_missing_export_file_is_a_french_error() {
        let directory = TempDir::new().expect("temp exports dir");
        let export = crate::platform::export::DocumentExport {
            pdf_path: directory.path().join("devis-10.pdf"),
            png_path: directory.path().join("devis-10.png"),
        };

        let Err(error) =
            attachment_from_export(&export, &DocumentKind::Quote, 10, ShareFormat::Pdf)
        else {
            panic!("a missing export file must fail");
        };

        assert!(error.contains("Impossible de lire le fichier"));
    }

    #[test]
    fn the_send_config_requires_stored_credentials() {
        let (_file, database) = temp_connection();
        let connection = lock(&database);
        let error = load_send_config(&connection).expect_err("unconfigured");
        assert!(error.contains("configurez la clé Brevo"));

        save_email_settings(&connection, Some("key"), "contact@variete-saveurs.fr", "")
            .expect("save settings");
        let config = load_send_config(&connection).expect("configured");
        assert_eq!(config.sender_email, "contact@variete-saveurs.fr");
    }

    struct MockMailer {
        outcome: Result<(), crate::domain::email::MailError>,
    }

    impl Mailer for MockMailer {
        fn send_document_email(
            &self,
            _config: &crate::domain::email::MailConfig,
            _to: &str,
            _subject: &str,
            _body_html: &str,
            _attachment: &crate::domain::email::Attachment,
        ) -> Result<(), crate::domain::email::MailError> {
            self.outcome.clone()
        }
    }

    #[test]
    fn a_successful_send_marks_the_document_sent_once() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let quote = issue_document(
            &mut write,
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        let mailer = MockMailer { outcome: Ok(()) };
        let config = MailConfig {
            api_key: "key".to_string(),
            sender_email: "contact@variete-saveurs.fr".to_string(),
            sender_name: None,
        };
        let attachment = crate::domain::email::Attachment {
            name: "devis-10.pdf".to_string(),
            bytes: vec![1],
        };

        // First send: the mailer accepts, then sent_at is written.
        mailer
            .send_document_email(
                &config,
                "marie@example.fr",
                "sujet",
                "<p>x</p>",
                &attachment,
            )
            .expect("first send");
        mark_sent_after_send(&write, quote.id, "2026-07-25T09:00:00Z").expect("mark first send");
        let reloaded = get_document(&write, quote.id).expect("reload");
        assert_eq!(reloaded.sent_at.as_deref(), Some("2026-07-25T09:00:00Z"));

        // Resend (task 27: « renvois possibles sans changer le statut »):
        // the mailer accepts again, sent_at stays the FIRST send's.
        mailer
            .send_document_email(
                &config,
                "marie@example.fr",
                "sujet",
                "<p>x</p>",
                &attachment,
            )
            .expect("resend");
        mark_sent_after_send(&write, quote.id, "2026-07-26T09:00:00Z")
            .expect("resend marking is a no-op");
        let reloaded = get_document(&write, quote.id).expect("reload");
        assert_eq!(reloaded.sent_at.as_deref(), Some("2026-07-25T09:00:00Z"));
    }

    #[test]
    fn a_failed_send_leaves_sent_at_empty() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let quote = issue_document(
            &mut write,
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        let mailer = MockMailer {
            outcome: Err(crate::domain::email::MailError::Network),
        };
        let config = MailConfig {
            api_key: "key".to_string(),
            sender_email: "contact@variete-saveurs.fr".to_string(),
            sender_name: None,
        };
        let attachment = crate::domain::email::Attachment {
            name: "devis-10.pdf".to_string(),
            bytes: vec![1],
        };

        let error = mailer
            .send_document_email(
                &config,
                "marie@example.fr",
                "sujet",
                "<p>x</p>",
                &attachment,
            )
            .expect_err("network failure");
        // The French message is what the persistent block shows; sent_at is
        // never written on the failure path (mark is only called after Ok).
        assert!(error.to_string().contains("connexion"));
        let reloaded = get_document(&write, quote.id).expect("reload");
        assert_eq!(reloaded.sent_at, None);
    }

    #[test]
    fn a_mark_failure_after_a_send_does_not_read_like_a_send_failure() {
        let (_file, database) = temp_connection();
        let mut write = lock(&database);
        let quote = issue_document(
            &mut write,
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        // A blank timestamp makes mark_sent fail — enough to check the
        // wording: the email IS gone, the message must not invite a resend.
        let error = mark_sent_after_send(&write, quote.id, "  ").expect_err("mark failure");

        assert!(error.contains("est parti"));
        assert!(!error.contains("envoi"));
    }
}
