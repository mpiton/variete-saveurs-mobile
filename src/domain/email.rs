//! Email sent with a document (ADR 0002, ARCHI §4): French branded content
//! built from `templates/email.html`, the sending configuration and the
//! `Mailer` abstraction. The domain stays network-free — the Brevo
//! implementation lives in `crate::platform::mail`, tests use a mock.

use std::fmt;

use html_escape::encode_text;
use thiserror::Error;

use super::models::DocumentKind;
use super::money::format_eur;

/// Branded transactional email (ADR 0002, ARCHI §4). Placeholders filled by
/// `render_email`: `{client_name}`, `{doc_label}`, `{total}` and
/// `{validity_paragraph}` — the latter is an insertion point for the whole
/// validity sentence, owned below so the conditional HTML lives in exactly
/// one place (a quote with a date gets the sentence, anything else gets
/// nothing). Table layout and inline styles only (email-client
/// compatibility); palette « Vitrine » (document register, DESIGN.md).
/// The logo is loaded remotely — Brevo does not support CID inline images
/// and Gmail blocks data URIs — from the public mobile repo until
/// variete-de-saveurs.fr is registered (web/ARCHI.md §5), then the `<img>`
/// URL should point at the website. The template ships as-is in emails:
/// no documentation comment with `{placeholder}` tokens belongs in it.
const TEMPLATE: &str = include_str!("../../templates/email.html");

/// Placeholder values for `templates/email.html`. The domain builds the
/// French labels itself so quote and invoice wording stays consistent
/// (task 26: « libellés adaptés » per kind).
pub struct EmailPlaceholders {
    /// Document number (« 10 » in « devis n° 10 »).
    pub doc_number: i64,
    pub client_name: String,
    pub total_cents: i64,
    /// French display date (« 24/08/2026 »); expected for quotes, ignored
    /// for invoices (their payment terms live on the document itself).
    pub validity_date: Option<String>,
}

/// What the compose screen starts from. The HTML body is not here: it is built
/// at send time by `render_email_html`, from whatever text she ended up with.
pub struct EmailContent {
    pub subject: String,
    /// Plain text, never markup: the paragraphs addressed to the client.
    /// `CLAUDE.md` puts a template editor out of scope — « modèle fixe, texte
    /// retouchable ». Handing the whole template to a textarea made the
    /// template the editable thing, and one keystroke inside a tag shipped a
    /// broken mail to a client.
    pub message: String,
}

/// Off-device archive address of a send (ADR 0002 — the BCC copy). A
/// `noreply@` sender is a mailbox nobody reads, so the archive goes to
/// `contact@` on the same domain instead; any other sender archives to
/// itself.
pub fn archive_address(sender_email: &str) -> String {
    let Some((local, domain)) = sender_email.split_once('@') else {
        return sender_email.to_string();
    };
    if local.eq_ignore_ascii_case("noreply") {
        format!("contact@{domain}")
    } else {
        sender_email.to_string()
    }
}

/// Sending configuration assembled from `settings` at send time
/// (`settings::load_email_credentials`) — the only path where the raw Brevo
/// key leaves storage.
pub struct MailConfig {
    pub api_key: String,
    pub sender_email: String,
    pub sender_name: Option<String>,
}

impl fmt::Debug for MailConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailConfig")
            .field("api_key", &"<redacted>")
            .field("sender_email", &self.sender_email)
            .field("sender_name", &self.sender_name)
            .finish()
    }
}

/// Document file attached to the email (PDF or PNG export). The extension
/// of `name` drives the MIME type at Brevo — there is no content-type field
/// in the API payload.
pub struct Attachment {
    /// « devis-10.pdf » / « facture-3.png ».
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Sending failures, worded for the gérante — never a raw HTTP code.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MailError {
    #[error(
        "La clé API Brevo est invalide ou n'a plus les droits d'envoi. Vérifiez-la dans Réglages."
    )]
    InvalidApiKey,
    #[error("Impossible de joindre le service d'envoi. Vérifiez la connexion et réessayez.")]
    Network,
    #[error("L'envoi a été refusé : {0}")]
    ApiRejected(String),
    #[error("La pièce jointe dépasse la taille maximale autorisée (4 Mo).")]
    AttachmentTooLarge,
}

/// One call, one verdict (ARCHI §4: no queue, no retry in v1). Implemented
/// by the Brevo client in `crate::platform::mail`; mocked in tests.
pub trait Mailer {
    fn send_document_email(
        &self,
        config: &MailConfig,
        to: &str,
        subject: &str,
        body_html: &str,
        attachment: &Attachment,
    ) -> Result<(), MailError>;
}

/// Builds the French subject (« Devis n° 10 — Variété de Saveurs ») and the
/// branded HTML body. User-entered values are HTML-escaped.
pub fn render_email(kind: &DocumentKind, values: &EmailPlaceholders) -> EmailContent {
    let message = default_message(kind, values);
    EmailContent {
        subject: format!(
            "{} n° {} — Variété de Saveurs",
            kind.label(),
            values.doc_number
        ),
        message,
    }
}

/// The pre-filled text she may retouch before sending. Plain text: the fixed
/// template wraps it at send time.
pub fn default_message(kind: &DocumentKind, values: &EmailPlaceholders) -> String {
    format!(
        "Vous trouverez ci-joint votre {} d'un montant de {}.\n\nN'hésitez pas à nous solliciter pour toute question.",
        doc_label(kind, values.doc_number),
        format_eur(values.total_cents)
    )
}

/// Wraps her plain text into the fixed template. Blank lines split paragraphs,
/// single newlines become `<br>`, and everything she typed is escaped.
pub fn render_email_html(kind: &DocumentKind, values: &EmailPlaceholders, message: &str) -> String {
    let validity_paragraph = match (kind, non_empty(values.validity_date.as_deref())) {
        (DocumentKind::Quote, Some(date)) => format!(
            "<p class=\"email-validity\">Cette offre est valable jusqu'au {}.</p>",
            encode_text(date)
        ),
        _ => String::new(),
    };

    // One pass, because chained `replace` calls are not one: each of them
    // re-scans what the previous ones inserted, so a client named « {message} »
    // was substituted into the greeting and then expanded by the next call,
    // dropping the whole message block onto the « Bonjour » line. Escaping does
    // not help — `encode_text` escapes HTML, not braces.
    fill(
        TEMPLATE,
        &[
            ("{validity_paragraph}", &validity_paragraph),
            ("{client_name}", &encode_text(&values.client_name)),
            ("{message}", &message_html(message)),
        ],
    )
}

/// Substitutes every placeholder in a single left-to-right pass: what a value
/// carries lands in the output and is never looked at again.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(brace) = rest.find('{') {
        out.push_str(&rest[..brace]);
        rest = &rest[brace..];
        match values
            .iter()
            .find_map(|(token, value)| rest.strip_prefix(token).map(|tail| (*value, tail)))
        {
            Some((value, tail)) => {
                out.push_str(value);
                rest = tail;
            }
            // An unknown `{` is literal text — the template owns its tokens.
            None => {
                out.push('{');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn message_html(message: &str) -> String {
    message
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| {
            format!(
                "<p style=\"margin:0 0 16px 0;\">{}</p>",
                encode_text(paragraph).replace('\n', "<br>")
            )
        })
        .collect::<Vec<_>>()
        .join("\n              ")
}

fn doc_label(kind: &DocumentKind, number: i64) -> String {
    format!("{} n° {number}", kind.label().to_lowercase())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{
        Attachment, EmailPlaceholders, MailConfig, MailError, Mailer, archive_address,
        default_message, render_email, render_email_html,
    };
    use crate::domain::models::DocumentKind;

    fn quote_values() -> EmailPlaceholders {
        EmailPlaceholders {
            doc_number: 10,
            client_name: "Marie Dupont".to_string(),
            total_cents: 139_435,
            validity_date: Some("24/08/2026".to_string()),
        }
    }

    /// The body as it goes out: her message wrapped in the fixed template.
    fn rendered(kind: &DocumentKind, values: &EmailPlaceholders) -> String {
        render_email_html(kind, values, &default_message(kind, values))
    }

    #[test]
    fn quote_email_fills_every_placeholder_and_keeps_validity() {
        let content = render_email(&DocumentKind::Quote, &quote_values());
        let body = rendered(&DocumentKind::Quote, &quote_values());

        assert_eq!(content.subject, "Devis n° 10 — Variété de Saveurs");
        assert!(body.contains("devis n° 10"));
        assert!(body.contains("Marie Dupont"));
        assert!(body.contains(&crate::domain::money::format_eur(139_435)));
        assert!(body.contains("valable jusqu'au 24/08/2026"));
    }

    #[test]
    fn the_editable_message_carries_no_markup() {
        // The whole point of the split: she edits sentences, not a template.
        let message = default_message(&DocumentKind::Quote, &quote_values());

        assert!(message.contains("devis n° 10"));
        assert!(!message.contains('<'));
        assert!(!message.contains('{'));
    }

    #[test]
    fn invoice_email_drops_the_validity_line() {
        let values = EmailPlaceholders {
            doc_number: 3,
            validity_date: None,
            ..quote_values()
        };
        let content = render_email(&DocumentKind::Invoice, &values);
        let body = rendered(&DocumentKind::Invoice, &values);

        assert_eq!(content.subject, "Facture n° 3 — Variété de Saveurs");
        assert!(body.contains("facture n° 3"));
        assert!(!body.contains("valable jusqu'au"));
        assert!(!body.contains("email-validity"));
    }

    #[test]
    fn quote_without_a_validity_date_drops_the_line_instead_of_dangling() {
        let body = rendered(
            &DocumentKind::Quote,
            &EmailPlaceholders {
                validity_date: None,
                ..quote_values()
            },
        );

        assert!(!body.contains("valable jusqu'au"));
    }

    #[test]
    fn email_escapes_user_text() {
        let values = EmailPlaceholders {
            client_name: "<script>alert(1)</script>".to_string(),
            ..quote_values()
        };
        let body = rendered(&DocumentKind::Quote, &values);

        assert!(!body.contains("<script>"));
        assert!(body.contains("&lt;script&gt;"));
    }

    #[test]
    fn a_retouched_message_cannot_smuggle_markup_in() {
        let body = render_email_html(
            &DocumentKind::Quote,
            &quote_values(),
            "<script>alert(1)</script>",
        );

        assert!(!body.contains("<script>"));
        assert!(body.contains("&lt;script&gt;"));
    }

    #[test]
    fn client_name_cannot_retrigger_placeholder_substitution() {
        // `{message}` on purpose: it is a *live* token substituted after the
        // client name. The previous version of this test used `{total}`, which
        // stopped being a token when the body moved to plain text, so it passed
        // no matter what the substitution did.
        let values = EmailPlaceholders {
            client_name: "{message}".to_string(),
            ..quote_values()
        };
        let body = render_email_html(&DocumentKind::Quote, &values, "Le corps du message.");

        assert!(body.contains("Bonjour {message},"));
        // And the real message still landed exactly once, where it belongs.
        assert_eq!(body.matches("Le corps du message.").count(), 1);
    }

    #[test]
    fn rendered_email_leaves_no_unfilled_placeholder() {
        for kind in [DocumentKind::Quote, DocumentKind::Invoice] {
            let body = rendered(&kind, &quote_values());
            for placeholder in [
                "{client_name}",
                "{message}",
                "{validity_date}",
                "{validity_paragraph}",
            ] {
                assert!(
                    !body.contains(placeholder),
                    "{kind:?} body still contains {placeholder}"
                );
            }
        }
    }

    struct MockMailer {
        recorded: RefCell<Vec<String>>,
        outcome: Result<(), MailError>,
    }

    impl Mailer for MockMailer {
        fn send_document_email(
            &self,
            _config: &MailConfig,
            to: &str,
            _subject: &str,
            _body_html: &str,
            _attachment: &Attachment,
        ) -> Result<(), MailError> {
            self.recorded.borrow_mut().push(to.to_string());
            self.outcome.clone()
        }
    }

    #[test]
    fn mock_mailer_records_calls_and_replays_the_outcome() {
        let mailer = MockMailer {
            recorded: RefCell::new(Vec::new()),
            outcome: Ok(()),
        };
        let config = MailConfig {
            api_key: "key".to_string(),
            sender_email: "contact@variete-de-saveurs.fr".to_string(),
            sender_name: None,
        };
        let attachment = Attachment {
            name: "devis-10.pdf".to_string(),
            bytes: vec![1, 2, 3],
        };

        mailer
            .send_document_email(
                &config,
                "client@example.fr",
                "sujet",
                "<p>corps</p>",
                &attachment,
            )
            .expect("send");

        assert_eq!(mailer.recorded.borrow().as_slice(), ["client@example.fr"]);
    }

    #[test]
    fn mail_errors_speak_french_without_http_codes() {
        assert!(
            MailError::InvalidApiKey
                .to_string()
                .contains("clé API Brevo")
        );
        assert!(MailError::Network.to_string().contains("connexion"));
        let rejected = MailError::ApiRejected("adresse destinataire invalide".to_string());
        let message = rejected.to_string();
        assert!(message.contains("refusé"));
        assert!(message.contains("adresse destinataire invalide"));
        assert!(!message.contains("400"));
        assert!(
            MailError::AttachmentTooLarge
                .to_string()
                .contains("taille maximale")
        );
    }

    #[test]
    fn a_noreply_sender_archives_to_contact_on_the_same_domain() {
        assert_eq!(
            archive_address("noreply@variete-de-saveurs.fr"),
            "contact@variete-de-saveurs.fr"
        );
        assert_eq!(archive_address("NoReply@example.fr"), "contact@example.fr");
    }

    #[test]
    fn any_other_sender_archives_to_itself() {
        assert_eq!(
            archive_address("contact@variete-de-saveurs.fr"),
            "contact@variete-de-saveurs.fr"
        );
        assert_eq!(archive_address("marie@example.fr"), "marie@example.fr");
        // A malformed sender is kept as-is: it surfaces at send time anyway.
        assert_eq!(archive_address("sans-arobase"), "sans-arobase");
    }

    #[test]
    fn mail_config_debug_never_leaks_the_api_key() {
        let config = MailConfig {
            api_key: "xoxb-super-secret".to_string(),
            sender_email: "contact@variete-de-saveurs.fr".to_string(),
            sender_name: Some("Variété de Saveurs".to_string()),
        };

        let debug = format!("{config:?}");

        assert!(!debug.contains("xoxb-super-secret"));
        assert!(debug.contains("redacted"));
    }
}
