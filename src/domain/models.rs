use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Quote,
    Invoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Individual,
    Professional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientInput {
    pub kind: ClientKind,
    pub name: String,
    pub address: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub business_id: Option<String>,
    pub billing_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LineInput {
    pub group: Option<String>,
    pub description: String,
    pub quantity: i64,
    pub unit_price_cents: i64,
}

impl LineInput {
    pub fn amount_cents(&self) -> i64 {
        self.quantity.saturating_mul(self.unit_price_cents)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInput {
    pub kind: DocumentKind,
    pub issue_date: String,
    pub event_date: String,
    pub payment_terms: String,
    pub client: ClientInput,
    pub lines: Vec<LineInput>,
}

impl DocumentInput {
    pub fn total_cents(&self) -> i64 {
        self.lines
            .iter()
            .map(LineInput::amount_cents)
            .fold(0_i64, i64::saturating_add)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: i64,
    pub number: i64,
    pub input: DocumentInput,
    pub source_quote_id: Option<i64>,
    pub sent_at: Option<String>,
    pub created_at: String,
    pub is_invoiced: bool,
}

impl Document {
    pub fn is_sent(&self) -> bool {
        self.sent_at.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: Option<i64>,
    pub name: String,
    pub group_name: Option<String>,
    pub unit_price_cents: i64,
    pub unit: Option<String>,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::{ClientInput, ClientKind, Document, DocumentInput, DocumentKind, LineInput};

    #[test]
    fn totals_lines_in_cents() {
        let input = DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: "2026-07-01".to_string(),
            event_date: "2026-07-19".to_string(),
            payment_terms: "à réception".to_string(),
            client: ClientInput {
                kind: ClientKind::Individual,
                name: "Client".to_string(),
                address: "Adresse".to_string(),
                email: None,
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: vec![
                LineInput {
                    group: None,
                    description: "A".to_string(),
                    quantity: 50,
                    unit_price_cents: 85,
                },
                LineInput {
                    group: None,
                    description: "B".to_string(),
                    quantity: 25,
                    unit_price_cents: 80,
                },
            ],
        };
        assert_eq!(input.total_cents(), 6_250);
    }

    #[test]
    fn saturates_line_amount_on_overflow() {
        let line = LineInput {
            group: None,
            description: "A".to_string(),
            quantity: i64::MAX,
            unit_price_cents: 2,
        };

        assert_eq!(line.amount_cents(), i64::MAX);
    }

    #[test]
    fn saturates_document_total_on_overflow() {
        let input = DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: "2026-07-01".to_string(),
            event_date: "2026-07-19".to_string(),
            payment_terms: "à réception".to_string(),
            client: ClientInput {
                kind: ClientKind::Individual,
                name: "Client".to_string(),
                address: "Adresse".to_string(),
                email: None,
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: vec![
                LineInput {
                    group: None,
                    description: "A".to_string(),
                    quantity: i64::MAX,
                    unit_price_cents: 1,
                },
                LineInput {
                    group: None,
                    description: "B".to_string(),
                    quantity: 1,
                    unit_price_cents: 1,
                },
            ],
        };

        assert_eq!(input.total_cents(), i64::MAX);
    }

    #[test]
    fn represents_issued_document_with_composed_input_and_statuses() {
        let mut document = Document {
            id: 42,
            number: 10,
            input: DocumentInput {
                kind: DocumentKind::Quote,
                issue_date: "2026-07-01".to_string(),
                event_date: "2026-07-19".to_string(),
                payment_terms: "à réception".to_string(),
                client: ClientInput {
                    kind: ClientKind::Individual,
                    name: "Client".to_string(),
                    address: "Adresse".to_string(),
                    email: Some("client@example.com".to_string()),
                    phone: None,
                    business_id: None,
                    billing_address: None,
                },
                lines: vec![LineInput {
                    group: None,
                    description: "A".to_string(),
                    quantity: 1,
                    unit_price_cents: 850,
                }],
            },
            source_quote_id: None,
            sent_at: Some("2026-07-02T10:30:00Z".to_string()),
            created_at: "2026-07-01T09:00:00Z".to_string(),
            is_invoiced: true,
        };

        assert_eq!(document.input.total_cents(), 850);
        assert!(document.is_invoiced);
        assert!(document.is_sent());

        document.sent_at = None;
        assert!(!document.is_sent());
    }
}
