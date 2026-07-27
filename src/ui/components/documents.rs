use dioxus::prelude::*;

use crate::domain::{models::DocumentInput, money::format_eur};

/// Names a draft in one line, for the places that offer to resume it or to
/// destroy it.
///
/// The three « Remplacer le brouillon ? » sheets each described what was
/// *arriving* and never what was leaving — the only thing actually lost, and
/// there is no undo anywhere in the app. What identifies a draft to her is who
/// it is for and what it comes to; the kind alone was also all the resume card
/// showed.
pub fn draft_summary(input: &DocumentInput) -> String {
    let kind = input.kind.label();
    let total = format_eur(input.total_cents());
    let client = input.client.name.trim();
    if client.is_empty() {
        format!("{kind} — {total}")
    } else {
        format!("{kind} pour {client} — {total}")
    }
}

#[cfg(test)]
mod draft_summary_tests {
    use super::draft_summary;
    use crate::domain::models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput};

    fn draft(name: &str) -> DocumentInput {
        DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: "2026-07-27".to_string(),
            event_date: String::new(),
            payment_terms: String::new(),
            client: ClientInput {
                kind: ClientKind::Individual,
                name: name.to_string(),
                address: String::new(),
                email: None,
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
    fn a_draft_is_named_by_who_it_is_for_and_what_it_comes_to() {
        assert_eq!(
            draft_summary(&draft("Mairie de Lyon")),
            "Devis pour Mairie de Lyon — 35,00 €"
        );
    }

    /// A draft created from home and left untouched has no client yet. « pour »
    /// with nothing after it reads as a bug, so the clause goes rather than
    /// standing empty.
    #[test]
    fn a_draft_without_a_client_drops_the_clause_instead_of_leaving_it_empty() {
        assert_eq!(draft_summary(&draft("   ")), "Devis — 35,00 €");
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BadgeKind {
    Sent,
    Invoiced,
}

impl BadgeKind {
    const fn class(self) -> &'static str {
        match self {
            Self::Sent => "status-badge--sent",
            Self::Invoiced => "status-badge--invoiced",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Sent => "envoyé",
            Self::Invoiced => "facturé",
        }
    }
}

#[component]
pub fn StatusBadge(kind: BadgeKind) -> Element {
    let class = format!("status-badge {}", kind.class());
    let label = kind.label();

    rsx! {
        span { class, "{label}" }
    }
}

#[component]
pub fn DocumentCard(
    document_type: String,
    number: i64,
    client: String,
    total: String,
    /// French issue date. The list carried no date at all and has no ceiling,
    /// so « le devis de la semaine dernière » was a scroll.
    issue_date: String,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] sent: bool,
    #[props(default)] invoiced: bool,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
) -> Element {
    let statuses = status_suffix(sent, invoiced);
    let accessible_label =
        format!("{document_type} numéro {number}, {client}, {issue_date}, total {total}{statuses}");

    rsx! {
        button {
            class: "document-card",
            r#type: "button",
            aria_label: accessible_label,
            aria_busy: loading,
            disabled: disabled || loading,
            onclick: move |event| onclick.call(event),
            span { class: "document-card__content", aria_hidden: loading,
                span { class: "document-card__heading",
                    strong { "{document_type} n° {number}" }
                    strong { class: "document-card__total", "{total}" }
                }
                span { class: "document-card__client", "{client}" }
                span { class: "document-card__meta",
                    span { class: "document-card__date", "{issue_date}" }
                    if sent || invoiced {
                        span { class: "document-card__badges",
                            if sent {
                                StatusBadge { kind: BadgeKind::Sent }
                            }
                            if invoiced {
                                StatusBadge { kind: BadgeKind::Invoiced }
                            }
                        }
                    }
                }
            }
            if loading {
                span { class: "spinner", aria_hidden: "true" }
            }
        }
    }
}

fn status_suffix(sent: bool, invoiced: bool) -> String {
    match (sent, invoiced) {
        (true, true) => format!(
            ", {}, {}",
            BadgeKind::Sent.label(),
            BadgeKind::Invoiced.label()
        ),
        (true, false) => format!(", {}", BadgeKind::Sent.label()),
        (false, true) => format!(", {}", BadgeKind::Invoiced.label()),
        (false, false) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::status_suffix;

    #[test]
    fn accessible_status_suffix_lists_each_visible_badge() {
        assert_eq!(status_suffix(false, false), "");
        assert_eq!(status_suffix(true, false), ", envoyé");
        assert_eq!(status_suffix(false, true), ", facturé");
        assert_eq!(status_suffix(true, true), ", envoyé, facturé");
    }
}
