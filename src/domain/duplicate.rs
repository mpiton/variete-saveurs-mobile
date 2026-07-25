//! Duplication of an issued document (CONTEXT.md « dupliquer », ARCHI §4):
//! correcting an issued document means duplicating it — a deep copy back
//! into the draft, without a number and without `source_quote_id`, re-dated
//! at the duplication day. The copy goes through the normal issue flow,
//! which assigns it a fresh number; the original stays frozen.

use super::models::{Document, DocumentInput};

/// Builds the draft copy of an issued `document` — same kind, client, lines
/// and payment terms deep-copied, no number and no `source_quote_id` (the
/// copy keeps no link with the original and marks nothing « facturé »).
/// `today` is injected (YYYY-MM-DD — no wall clock in the domain): both
/// dates are reset to the duplication day and the gérante adjusts any field
/// before issuing; the normal issue flow assigns the copy a fresh number.
pub fn duplicate_draft_from_document(document: &Document, today: &str) -> DocumentInput {
    DocumentInput {
        kind: document.input.kind.clone(),
        issue_date: today.to_string(),
        event_date: today.to_string(),
        payment_terms: document.input.payment_terms.clone(),
        client: document.input.client.clone(),
        lines: document.input.lines.clone(),
        source_quote_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::duplicate_draft_from_document;
    use crate::domain::models::{
        ClientInput, ClientKind, Document, DocumentInput, DocumentKind, LineInput,
    };

    fn issued_document(kind: DocumentKind) -> Document {
        Document {
            id: 7,
            number: 10,
            input: DocumentInput {
                kind,
                issue_date: "2026-07-01".to_string(),
                event_date: "2026-07-19".to_string(),
                payment_terms: "à réception".to_string(),
                client: ClientInput {
                    kind: ClientKind::Professional,
                    name: "Mairie de Lyon".to_string(),
                    address: "12 rue Émile Zola, Lyon".to_string(),
                    email: Some("contact@example.com".to_string()),
                    phone: None,
                    business_id: Some("123 456 789 00010".to_string()),
                    billing_address: None,
                },
                lines: vec![
                    LineInput {
                        group: Some("Salé".to_string()),
                        description: "Mini burgers".to_string(),
                        quantity: 50,
                        unit_price_cents: 85,
                    },
                    LineInput {
                        group: None,
                        description: "Mini brochettes de fruits".to_string(),
                        quantity: 10,
                        unit_price_cents: 85,
                    },
                ],
                source_quote_id: None,
            },
            total_cents: 5_100,
            source_quote_id: None,
            sent_at: None,
            created_at: "2026-07-01T09:00:00Z".to_string(),
            is_invoiced: false,
        }
    }

    #[test]
    fn the_duplication_deep_copies_the_document_into_a_same_kind_draft() {
        let quote = issued_document(DocumentKind::Quote);

        let draft = duplicate_draft_from_document(&quote, "2026-07-25");

        assert_eq!(draft.kind, DocumentKind::Quote);
        assert_eq!(draft.payment_terms, quote.input.payment_terms);
        assert_eq!(draft.client, quote.input.client);
        assert_eq!(draft.lines, quote.input.lines);
    }

    #[test]
    fn the_duplication_resets_both_dates_to_today() {
        let quote = issued_document(DocumentKind::Quote);

        let draft = duplicate_draft_from_document(&quote, "2026-07-25");

        assert_eq!(draft.issue_date, "2026-07-25");
        assert_eq!(draft.event_date, "2026-07-25");
    }

    #[test]
    fn the_duplication_keeps_no_link_with_the_original() {
        let mut invoice = issued_document(DocumentKind::Invoice);
        invoice.input.source_quote_id = Some(3);
        invoice.source_quote_id = Some(3);

        let draft = duplicate_draft_from_document(&invoice, "2026-07-25");

        assert_eq!(draft.kind, DocumentKind::Invoice);
        assert_eq!(draft.source_quote_id, None);
    }

    #[test]
    fn the_duplication_copy_is_independent_from_the_document() {
        let mut quote = issued_document(DocumentKind::Quote);
        let mut draft = duplicate_draft_from_document(&quote, "2026-07-25");

        draft.lines[0].quantity = 60;
        draft.client.name = "Ajusté".to_string();
        quote.input.lines.push(LineInput {
            group: None,
            description: "Ajout ultérieur".to_string(),
            quantity: 1,
            unit_price_cents: 100,
        });

        assert_eq!(quote.input.lines.len(), 3);
        assert_eq!(draft.lines.len(), 2);
        assert_eq!(quote.input.lines[0].quantity, 50);
        assert_eq!(quote.input.client.name, "Mairie de Lyon");
    }
}
