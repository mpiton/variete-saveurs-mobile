//! Brevo transactional email client (ADR 0002, ARCHI §4): one POST to
//! `/v3/smtp/email`, one verdict — no queue, no retry in v1. Blocking
//! reqwest over rustls, called from a worker thread like the export/share
//! paths; TLS uses webpki roots so nothing reads the device trust store.
//! The API key travels in the `api-key` header and is never logged.

use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::domain::email::{Attachment, MailConfig, MailError, Mailer, archive_address};

const API_URL: &str = "https://api.brevo.com/v3/smtp/email";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Brevo caps each attachment at 4 MB and base64 inflates bytes by 4/3;
/// refusing oversized files client-side (« taille contrôlée », task 26)
/// avoids a pointless round-trip. 2.9 MB raw encodes to ~3.87 MB, under
/// the limit whether Brevo counts binary or decimal megabytes.
const MAX_ATTACHMENT_BYTES: usize = 2_900_000;

/// HTTP client for the Brevo API. Stateless: configuration arrives with
/// each call so nothing secret is retained.
pub struct BrevoMailer;

impl Mailer for BrevoMailer {
    fn send_document_email(
        &self,
        config: &MailConfig,
        to: &str,
        subject: &str,
        body_html: &str,
        attachment: &Attachment,
    ) -> Result<(), MailError> {
        if attachment.bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(MailError::AttachmentTooLarge);
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            // A fixed API endpoint never legitimately redirects; following
            // one would re-POST the `api-key` header to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MailError::Network)?;
        let archive = archive_address(&config.sender_email);
        let payload = build_payload(config, to, subject, body_html, attachment, &archive);
        let response = client
            .post(API_URL)
            .header("api-key", &config.api_key)
            .json(&payload)
            .send()
            .map_err(|_| MailError::Network)?;
        let status = response.status().as_u16();
        let body = response.text().unwrap_or_default();
        interpret_response(status, &body)
    }
}

#[derive(Serialize)]
struct SendPayload<'a> {
    sender: Contact<'a>,
    to: Vec<Contact<'a>>,
    /// Every send is copied to the archive address (`archive_address` —
    /// `contact@` for a `noreply@` sender, the sender itself otherwise):
    /// the off-device archive that compensates for Brevo not filling the
    /// « Sent » folder (ADR 0002).
    bcc: Vec<Contact<'a>>,
    subject: &'a str,
    #[serde(rename = "htmlContent")]
    html_content: &'a str,
    attachment: Vec<AttachmentPayload>,
}

#[derive(Serialize)]
struct Contact<'a> {
    email: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

/// Brevo attachment: `name` (whose extension drives the MIME type) and
/// base64 `content` — the API has no content-type field.
#[derive(Serialize)]
struct AttachmentPayload {
    name: String,
    content: String,
}

fn build_payload<'a>(
    config: &'a MailConfig,
    to: &'a str,
    subject: &'a str,
    body_html: &'a str,
    attachment: &'a Attachment,
    archive: &'a str,
) -> SendPayload<'a> {
    let sender = Contact {
        email: &config.sender_email,
        name: config.sender_name.as_deref(),
    };
    SendPayload {
        sender,
        to: vec![Contact {
            email: to,
            name: None,
        }],
        bcc: vec![Contact {
            email: archive,
            name: None,
        }],
        subject,
        html_content: body_html,
        attachment: vec![AttachmentPayload {
            name: attachment.name.clone(),
            content: base64::engine::general_purpose::STANDARD.encode(&attachment.bytes),
        }],
    }
}

/// Maps the API verdict to a French error. Pure so it is testable without
/// any network (CLAUDE.md NEVER).
fn interpret_response(status: u16, body: &str) -> Result<(), MailError> {
    match status {
        200..=299 => Ok(()),
        // Only 401 means « bad key »; 403 also covers unvalidated senders
        // or suspended accounts, whose Brevo message is more useful.
        401 => Err(MailError::InvalidApiKey),
        _ => Err(MailError::ApiRejected(rejection_reason(body))),
    }
}

/// Brevo errors are `{"code": "...", "message": "..."}`; only the message
/// reaches the gérante, wrapped in French — never a bare HTTP status.
fn rejection_reason(body: &str) -> String {
    #[derive(Deserialize)]
    struct BrevoError {
        message: String,
    }

    serde_json::from_str::<BrevoError>(body)
        .map(|error| error.message)
        .unwrap_or_else(|_| "réponse inattendue du service, réessayez plus tard.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{BrevoMailer, MAX_ATTACHMENT_BYTES, build_payload, interpret_response};
    use crate::domain::email::{Attachment, MailConfig, MailError, Mailer, archive_address};

    fn config() -> MailConfig {
        MailConfig {
            api_key: "test-key".to_string(),
            sender_email: "contact@variete-de-saveurs.fr".to_string(),
            sender_name: Some("Variété de Saveurs".to_string()),
        }
    }

    fn attachment(bytes: Vec<u8>) -> Attachment {
        Attachment {
            name: "devis-10.pdf".to_string(),
            bytes,
        }
    }

    #[test]
    fn accepted_status_is_a_success() {
        assert!(interpret_response(201, r#"{"messageId":"<abc@relay.brevo.com>"}"#).is_ok());
    }

    #[test]
    fn unauthorized_maps_to_a_french_key_error() {
        assert_eq!(
            interpret_response(401, r#"{"message":"Key not found","code":"unauthorized"}"#),
            Err(MailError::InvalidApiKey)
        );
    }

    #[test]
    fn forbidden_surfaces_the_brevo_message_not_a_key_error() {
        let result = interpret_response(
            403,
            r#"{"code":"permission_denied","message":"Sender domain not validated"}"#,
        );

        let Err(MailError::ApiRejected(reason)) = result else {
            panic!("403 must be an ApiRejected, got {result:?}");
        };
        assert!(reason.contains("Sender domain not validated"));
    }

    #[test]
    fn api_refusal_surfaces_the_brevo_message_not_the_http_code() {
        let result = interpret_response(
            400,
            r#"{"code":"invalid_parameter","message":"Invalid email address"}"#,
        );

        let Err(MailError::ApiRejected(reason)) = result else {
            panic!("400 must be an ApiRejected, got {result:?}");
        };
        assert!(reason.contains("Invalid email address"));
        assert!(!reason.contains("400"));
    }

    #[test]
    fn unparseable_error_body_gets_a_generic_french_reason() {
        let result = interpret_response(500, "<html>Bad Gateway</html>");

        let Err(MailError::ApiRejected(reason)) = result else {
            panic!("500 must be an ApiRejected, got {result:?}");
        };
        assert!(reason.contains("réessayez plus tard"));
        assert!(!reason.contains("500"));
    }

    #[test]
    fn payload_copies_the_archive_in_bcc_and_encodes_the_attachment() {
        let config = config();
        let attachment = attachment(b"hello".to_vec());
        let archive = archive_address(&config.sender_email);
        let payload = build_payload(
            &config,
            "client@example.fr",
            "Devis n° 10 — Variété de Saveurs",
            "<p>corps</p>",
            &attachment,
            &archive,
        );
        let json = serde_json::to_value(&payload).expect("serialize payload");

        assert_eq!(json["sender"]["email"], "contact@variete-de-saveurs.fr");
        assert_eq!(json["sender"]["name"], "Variété de Saveurs");
        assert_eq!(json["to"][0]["email"], "client@example.fr");
        assert_eq!(json["bcc"][0]["email"], "contact@variete-de-saveurs.fr");
        assert_eq!(json["subject"], "Devis n° 10 — Variété de Saveurs");
        assert_eq!(json["htmlContent"], "<p>corps</p>");
        assert_eq!(json["attachment"][0]["name"], "devis-10.pdf");
        assert_eq!(json["attachment"][0]["content"], "aGVsbG8=");
        // The API key must never appear in the body — only in the header.
        assert!(!json.to_string().contains("test-key"));
    }

    #[test]
    fn a_noreply_sender_archives_the_copy_to_contact() {
        let mut config = config();
        config.sender_email = "noreply@variete-de-saveurs.fr".to_string();
        let attachment = attachment(vec![1]);
        let archive = archive_address(&config.sender_email);
        let payload = build_payload(
            &config,
            "c@example.fr",
            "s",
            "<p>x</p>",
            &attachment,
            &archive,
        );
        let json = serde_json::to_value(&payload).expect("serialize payload");

        assert_eq!(json["sender"]["email"], "noreply@variete-de-saveurs.fr");
        assert_eq!(json["bcc"][0]["email"], "contact@variete-de-saveurs.fr");
    }

    #[test]
    fn sender_name_is_omitted_from_the_json_when_unset() {
        let mut config = config();
        config.sender_name = None;
        let attachment = attachment(vec![1]);
        let archive = archive_address(&config.sender_email);
        let payload = build_payload(
            &config,
            "c@example.fr",
            "s",
            "<p>x</p>",
            &attachment,
            &archive,
        );
        let json = serde_json::to_value(&payload).expect("serialize payload");

        assert!(json["sender"].get("name").is_none());
    }

    #[test]
    fn oversized_attachment_is_refused_before_any_network_call() {
        let mailer = BrevoMailer;
        let result = mailer.send_document_email(
            &config(),
            "client@example.fr",
            "sujet",
            "<p>corps</p>",
            &attachment(vec![0_u8; MAX_ATTACHMENT_BYTES + 1]),
        );

        assert_eq!(result, Err(MailError::AttachmentTooLarge));
    }
}
