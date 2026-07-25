//! Quote → invoice conversion (CONTEXT.md « Conversion », ARCHI §4): the
//! invoice draft is a deep, fully editable copy of the issued quote —
//! client, lines, event date — re-dated at the conversion day. It carries
//! the quote's id up to the emission, which marks the quote « facturé »
//! (derived status — the quote itself is never written).

use super::models::{Document, DocumentInput, DocumentKind};

/// Builds the pre-filled invoice draft from an issued `quote`. `issue_date`
/// is injected (YYYY-MM-DD — no wall clock in the domain): the UI dates the
/// invoice « today » and the gérante adjusts any field before issuing.
pub fn invoice_draft_from_quote(quote: &Document, issue_date: &str) -> DocumentInput {
    DocumentInput {
        kind: DocumentKind::Invoice,
        issue_date: issue_date.to_string(),
        event_date: quote.input.event_date.clone(),
        payment_terms: quote.input.payment_terms.clone(),
        client: quote.input.client.clone(),
        lines: quote.input.lines.clone(),
        source_quote_id: Some(quote.id),
    }
}

#[cfg(test)]
mod tests {
    use super::invoice_draft_from_quote;
    use crate::domain::models::{
        ClientInput, ClientKind, Document, DocumentInput, DocumentKind, LineInput,
    };

    fn issued_quote() -> Document {
        Document {
            id: 7,
            number: 10,
            input: DocumentInput {
                kind: DocumentKind::Quote,
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
    fn the_conversion_deep_copies_the_quote_into_an_invoice_draft() {
        let quote = issued_quote();

        let draft = invoice_draft_from_quote(&quote, "2026-07-22");

        assert_eq!(draft.kind, DocumentKind::Invoice);
        assert_eq!(draft.issue_date, "2026-07-22");
        assert_eq!(draft.event_date, quote.input.event_date);
        assert_eq!(draft.payment_terms, quote.input.payment_terms);
        assert_eq!(draft.client, quote.input.client);
        assert_eq!(draft.lines, quote.input.lines);
        assert_eq!(draft.source_quote_id, Some(quote.id));
    }

    #[test]
    fn the_conversion_copy_is_independent_from_the_quote() {
        let mut quote = issued_quote();
        let mut draft = invoice_draft_from_quote(&quote, "2026-07-22");

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
