use chrono::NaiveDate;

use super::models::{CatalogItem, DocumentInput, DocumentKind};

pub const MAX_LINE_QUANTITY: i64 = 100_000;
pub const MAX_UNIT_PRICE_CENTS: i64 = 10_000_000;
const MAX_LINE_AMOUNT_CENTS: i64 = 100_000_000;
const MAX_DOCUMENT_TOTAL_CENTS: i64 = 100_000_000;

pub fn validate_document(input: &DocumentInput) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut total_cents = 0_i64;
    let mut total_over_limit = false;

    validate_date(&input.issue_date, "La date d'émission", &mut errors);
    validate_date(&input.event_date, "La date de l'événement", &mut errors);
    if matches!(input.kind, DocumentKind::Invoice) && input.payment_terms.trim().is_empty() {
        errors.push("Les conditions de paiement sont obligatoires.".to_string());
    }
    if input.client.name.trim().is_empty() {
        errors.push("Le nom du client est obligatoire.".to_string());
    }
    if input.client.address.trim().is_empty() {
        errors.push("L'adresse du client est obligatoire.".to_string());
    }
    if input.lines.is_empty() {
        errors.push("Ajoutez au moins une prestation.".to_string());
    }
    for (index, line) in input.lines.iter().enumerate() {
        if line.description.trim().is_empty() {
            errors.push(format!(
                "Ligne {}: la désignation est obligatoire.",
                index + 1
            ));
        }
        if line.quantity <= 0 {
            errors.push(format!(
                "Ligne {}: la quantité doit être positive.",
                index + 1
            ));
        }
        if line.quantity > MAX_LINE_QUANTITY {
            errors.push(format!(
                "Ligne {}: la quantité dépasse la limite autorisée.",
                index + 1
            ));
        }
        if line.unit_price_cents < 0 {
            errors.push(format!(
                "Ligne {}: le prix ne peut pas être négatif.",
                index + 1
            ));
        }
        if line.unit_price_cents > MAX_UNIT_PRICE_CENTS {
            errors.push(format!(
                "Ligne {}: le prix dépasse la limite autorisée.",
                index + 1
            ));
        }

        if line.quantity > 0
            && line.quantity <= MAX_LINE_QUANTITY
            && line.unit_price_cents >= 0
            && line.unit_price_cents <= MAX_UNIT_PRICE_CENTS
        {
            match line.quantity.checked_mul(line.unit_price_cents) {
                Some(amount_cents) if amount_cents <= MAX_LINE_AMOUNT_CENTS => {
                    match total_cents.checked_add(amount_cents) {
                        Some(new_total) => total_cents = new_total,
                        None => total_over_limit = true,
                    }
                }
                _ => errors.push(format!(
                    "Ligne {}: le montant dépasse la limite autorisée.",
                    index + 1
                )),
            }
        }
    }
    if total_over_limit || total_cents > MAX_DOCUMENT_TOTAL_CENTS {
        errors.push("Le total du document dépasse la limite autorisée.".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_date(value: &str, label: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{label} est obligatoire."));
        return;
    }

    if !is_iso_date_shape(value) || NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
        errors.push(format!("{label} est invalide."));
    }
}

fn is_iso_date_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..].iter().all(u8::is_ascii_digit)
}

pub fn validate_catalog_items(items: &[CatalogItem]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let line_number = index + 1;

        if item.name.trim().is_empty() {
            errors.push(format!("Article {line_number}: le nom est obligatoire."));
        }
        if item.unit_price_cents < 0 {
            errors.push(format!(
                "Article {line_number}: le prix ne peut pas être négatif."
            ));
        }
        if item.unit_price_cents > MAX_UNIT_PRICE_CENTS {
            errors.push(format!(
                "Article {line_number}: le prix dépasse la limite autorisée."
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_catalog_items, validate_document};
    use crate::domain::models::{
        CatalogItem, ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput,
    };

    fn valid_doc() -> DocumentInput {
        DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: "2026-07-01".to_string(),
            event_date: "2026-07-19".to_string(),
            payment_terms: "à réception".to_string(),
            client: ClientInput {
                kind: ClientKind::Individual,
                name: "Fred Choulet Sallien".to_string(),
                address: "1 rue Exemple, 17000 La Rochelle".to_string(),
                email: None,
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: vec![LineInput {
                group: Some("Salé".to_string()),
                description: "Mini Burgers".to_string(),
                quantity: 50,
                unit_price_cents: 85,
            }],
        }
    }

    #[test]
    fn accepts_complete_document() {
        assert!(validate_document(&valid_doc()).is_ok());
    }

    #[test]
    fn rejects_missing_client_and_lines() {
        let mut doc = valid_doc();
        doc.client.name.clear();
        doc.client.address.clear();
        doc.lines.clear();
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Le nom du client est obligatoire.".to_string()));
        assert!(errors.contains(&"L'adresse du client est obligatoire.".to_string()));
        assert!(errors.contains(&"Ajoutez au moins une prestation.".to_string()));
    }

    #[test]
    fn rejects_invalid_or_missing_dates() {
        let mut doc = valid_doc();
        doc.issue_date = " ".to_string();
        doc.event_date = "not-a-date".to_string();

        let errors = validate_document(&doc).unwrap_err();

        assert!(errors.contains(&"La date d'émission est obligatoire.".to_string()));
        assert!(errors.contains(&"La date de l'événement est invalide.".to_string()));
    }

    #[test]
    fn rejects_dates_that_do_not_use_iso_day_and_month_width() {
        for value in ["2026-7-1", "2026-07-1", " 2026-07-01 "] {
            let mut doc = valid_doc();
            doc.issue_date = value.to_string();

            let errors = validate_document(&doc).unwrap_err();

            assert!(
                errors.contains(&"La date d'émission est invalide.".to_string()),
                "{value:?} should be rejected, got {errors:?}"
            );
        }
    }

    #[test]
    fn rejects_blank_invoice_payment_terms() {
        let mut doc = valid_doc();
        doc.kind = DocumentKind::Invoice;
        doc.payment_terms = " ".to_string();

        let errors = validate_document(&doc).unwrap_err();

        assert!(errors.contains(&"Les conditions de paiement sont obligatoires.".to_string()));
    }

    #[test]
    fn rejects_blank_line_description() {
        let mut doc = valid_doc();
        doc.lines[0].description = "   ".to_string();
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: la désignation est obligatoire.".to_string()));
    }

    #[test]
    fn rejects_zero_quantity() {
        let mut doc = valid_doc();
        doc.lines[0].quantity = 0;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: la quantité doit être positive.".to_string()));
    }

    #[test]
    fn rejects_negative_quantity() {
        let mut doc = valid_doc();
        doc.lines[0].quantity = -1;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: la quantité doit être positive.".to_string()));
    }

    #[test]
    fn rejects_negative_unit_price() {
        let mut doc = valid_doc();
        doc.lines[0].unit_price_cents = -1;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: le prix ne peut pas être négatif.".to_string()));
    }

    #[test]
    fn reports_missing_professional_name_once() {
        let mut doc = valid_doc();
        doc.client.kind = ClientKind::Professional;
        doc.client.name.clear();
        let errors = validate_document(&doc).unwrap_err();
        assert_eq!(
            errors,
            vec!["Le nom du client est obligatoire.".to_string()]
        );
    }

    #[test]
    fn rejects_quantity_above_app_limit() {
        let mut doc = valid_doc();
        doc.lines[0].quantity = 100_001;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: la quantité dépasse la limite autorisée.".to_string()));
    }

    #[test]
    fn rejects_unit_price_above_app_limit() {
        let mut doc = valid_doc();
        doc.lines[0].unit_price_cents = 10_000_001;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: le prix dépasse la limite autorisée.".to_string()));
    }

    #[test]
    fn rejects_line_amount_above_app_limit() {
        let mut doc = valid_doc();
        doc.lines[0].quantity = 11;
        doc.lines[0].unit_price_cents = 10_000_000;
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Ligne 1: le montant dépasse la limite autorisée.".to_string()));
    }

    #[test]
    fn rejects_document_total_above_app_limit() {
        let mut doc = valid_doc();
        doc.lines = vec![
            LineInput {
                group: None,
                description: "A".to_string(),
                quantity: 6,
                unit_price_cents: 10_000_000,
            },
            LineInput {
                group: None,
                description: "B".to_string(),
                quantity: 5,
                unit_price_cents: 10_000_000,
            },
        ];
        let errors = validate_document(&doc).unwrap_err();
        assert!(errors.contains(&"Le total du document dépasse la limite autorisée.".to_string()));
    }

    #[test]
    fn rejects_invalid_catalog_items() {
        let errors = validate_catalog_items(&[CatalogItem {
            id: None,
            name: " ".to_string(),
            group_name: None,
            unit_price_cents: 10_000_001,
            unit: None,
            active: true,
        }])
        .unwrap_err();

        assert!(errors.contains(&"Article 1: le nom est obligatoire.".to_string()));
        assert!(errors.contains(&"Article 1: le prix dépasse la limite autorisée.".to_string()));
    }
}
