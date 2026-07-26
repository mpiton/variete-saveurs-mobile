use dioxus::prelude::*;
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
