use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Quote,
    Invoice,
}

impl DocumentKind {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Quote => "quote",
            Self::Invoice => "invoice",
        }
    }

    /// French display label (« Devis »/« Facture ») for screens and snackbars.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Quote => "Devis",
            Self::Invoice => "Facture",
        }
    }
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
    /// Set by a conversion (F12): the invoice draft carries the source quote
    /// up to the emission, which then marks the quote « facturé » (derived
    /// status, CONTEXT.md). Also populated on a loaded issued invoice.
    /// `default` so drafts persisted before this field still load.
    #[serde(default)]
    pub source_quote_id: Option<i64>,
}

impl DocumentInput {
    pub fn total_cents(&self) -> i64 {
        self.lines
            .iter()
            .map(LineInput::amount_cents)
            .fold(0_i64, i64::saturating_add)
    }

    /// Whether the draft payload carries any user-entered content: a date,
    /// a client detail, payment terms or a line (the home's blank draft
    /// leaves them all empty, and only deliberate input fills them). Kind
    /// never fills a draft — picking a kind carries no data.
    pub fn is_blank(&self) -> bool {
        let client = &self.client;
        self.issue_date.trim().is_empty()
            && self.event_date.trim().is_empty()
            && self.payment_terms.trim().is_empty()
            && self.lines.is_empty()
            && client.name.trim().is_empty()
            && client.address.trim().is_empty()
            && client
                .email
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && client
                .phone
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && client
                .business_id
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && client
                .billing_address
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: i64,
    pub number: i64,
    pub input: DocumentInput,
    pub total_cents: i64,
    pub source_quote_id: Option<i64>,
    pub sent_at: Option<String>,
    pub created_at: String,
    pub is_invoiced: bool,
}

impl Document {
    pub fn is_sent(&self) -> bool {
        self.sent_at
            .as_deref()
            .is_some_and(|sent_at| !sent_at.trim().is_empty())
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
    fn document_kind_label_is_the_french_display_name() {
        assert_eq!(DocumentKind::Quote.label(), "Devis");
        assert_eq!(DocumentKind::Invoice.label(), "Facture");
    }

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
            source_quote_id: None,
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
            source_quote_id: None,
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
                source_quote_id: None,
            },
            total_cents: 850,
            source_quote_id: None,
            sent_at: Some("2026-07-02T10:30:00Z".to_string()),
            created_at: "2026-07-01T09:00:00Z".to_string(),
            is_invoiced: true,
        };

        assert_eq!(document.input.total_cents(), 850);
        assert!(document.is_invoiced);
        assert!(document.is_sent());

        document.sent_at = Some("   ".to_string());
        assert!(!document.is_sent());

        document.sent_at = None;
        assert!(!document.is_sent());
    }

    fn blank_input() -> DocumentInput {
        DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: String::new(),
            event_date: String::new(),
            payment_terms: String::new(),
            client: ClientInput {
                kind: ClientKind::Individual,
                name: String::new(),
                address: String::new(),
                email: None,
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: Vec::new(),
            source_quote_id: None,
        }
    }

    #[test]
    fn a_draft_without_any_content_is_blank() {
        assert!(blank_input().is_blank());
    }

    #[test]
    fn kind_never_fills_a_blank_draft() {
        let mut input = blank_input();
        input.kind = DocumentKind::Invoice;

        assert!(input.is_blank());
    }

    #[test]
    fn an_entered_date_fills_the_draft() {
        let mut by_issue_date = blank_input();
        by_issue_date.issue_date = "2026-07-25".to_string();
        assert!(!by_issue_date.is_blank());

        let mut by_event_date = blank_input();
        by_event_date.event_date = "2026-08-02".to_string();
        assert!(!by_event_date.is_blank());
    }

    #[test]
    fn any_client_detail_fills_the_draft() {
        let mut by_name = blank_input();
        by_name.client.name = "Marie Dupont".to_string();
        assert!(!by_name.is_blank());

        let mut by_address = blank_input();
        by_address.client.address = "12 rue des Lilas".to_string();
        assert!(!by_address.is_blank());

        let mut by_email = blank_input();
        by_email.client.email = Some("marie@example.com".to_string());
        assert!(!by_email.is_blank());

        let mut by_phone = blank_input();
        by_phone.client.phone = Some("0600000000".to_string());
        assert!(!by_phone.is_blank());

        let mut by_business_id = blank_input();
        by_business_id.client.business_id = Some("123 456 789 00010".to_string());
        assert!(!by_business_id.is_blank());

        let mut by_billing_address = blank_input();
        by_billing_address.client.billing_address = Some("1 rue ailleurs".to_string());
        assert!(!by_billing_address.is_blank());
    }

    #[test]
    fn payment_terms_or_a_line_fill_the_draft() {
        let mut by_terms = blank_input();
        by_terms.payment_terms = "Comptant".to_string();
        assert!(!by_terms.is_blank());

        let mut by_line = blank_input();
        by_line.lines.push(LineInput {
            group: None,
            description: "Pains spéciaux".to_string(),
            quantity: 10,
            unit_price_cents: 350,
        });
        assert!(!by_line.is_blank());
    }

    #[test]
    fn whitespace_only_content_stays_blank() {
        let mut input = blank_input();
        input.client.name = "   ".to_string();
        input.payment_terms = " \n ".to_string();
        input.client.email = Some(" ".to_string());

        assert!(input.is_blank());
    }
}
