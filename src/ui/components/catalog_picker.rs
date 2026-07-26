//! Catalog picker bottom sheet: active items as two-column chips (name +
//! price, min 54px), grouped by `group_name`. One tap copies the item into
//! a draft line (quantity 1, editable afterwards like any typed line);
//! free-form entry stays available through the line sheet via « Saisie
//! libre » (CONTEXT.md: a line may come from the catalog or be typed).

use dioxus::prelude::*;

use crate::domain::{
    models::{CatalogItem, LineInput},
    money::format_eur,
};

use super::{
    actions::{Button, ButtonVariant},
    feedback::BottomSheet,
};

/// Heading for items without a group.
const UNGROUPED_TITLE: &str = "Autres";

/// Items sharing a `group_name`, in display order.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogGroup {
    pub name: Option<String>,
    pub items: Vec<CatalogItem>,
}

impl CatalogGroup {
    pub fn title(&self) -> &str {
        self.name.as_deref().unwrap_or(UNGROUPED_TITLE)
    }
}

/// Run-length grouping: consecutive items with the same group land together.
/// The input must be ordered by (group_name, name) — the db catalog queries
/// guarantee it.
pub fn group_catalog_items(items: Vec<CatalogItem>) -> Vec<CatalogGroup> {
    let mut groups: Vec<CatalogGroup> = Vec::new();
    for item in items {
        match groups.last_mut() {
            Some(group) if group.name == item.group_name => group.items.push(item),
            _ => groups.push(CatalogGroup {
                name: item.group_name.clone(),
                items: vec![item],
            }),
        }
    }
    groups
}

/// One tap on a chip copies the item into a draft line — quantity 1. The
/// line is copied, never referenced: a later catalog edit cannot rewrite a
/// draft line nor an issued document.
pub fn line_from_catalog_item(item: &CatalogItem) -> LineInput {
    LineInput {
        group: item.group_name.clone(),
        description: item.name.clone(),
        quantity: 1,
        unit_price_cents: item.unit_price_cents,
    }
}

#[component]
pub fn CatalogPicker(
    state: Signal<Option<Vec<CatalogItem>>>,
    on_pick: EventHandler<CatalogItem>,
    on_free_form: EventHandler<MouseEvent>,
) -> Element {
    // Picks accumulate instead of closing the sheet on each one: a traiteur
    // quote is five to ten items, and closing after every chip made each one
    // cost a reopen. The count is the receipt for taps that land behind the
    // scrim; it resets with the sheet.
    let mut added = use_signal(|| 0usize);
    let mut close = move || {
        added.set(0);
        state.set(None);
    };

    let Some(items) = state.read().clone() else {
        return rsx! {};
    };
    let groups = group_catalog_items(items);

    rsx! {
        BottomSheet {
            id: "catalog-picker".to_string(),
            title: "Ajouter une prestation".to_string(),
            open: true,
            on_dismiss: move |_| close(),
            if groups.is_empty() {
                p { "Aucun article actif au catalogue. Créez-le depuis l’écran Catalogue." }
            } else {
                for (group_index, group) in groups.into_iter().enumerate() {
                    section {
                        key: "{group_index}",
                        class: "catalog-picker__group",
                        aria_labelledby: "catalog-picker-group-{group_index}",
                        h3 { id: "catalog-picker-group-{group_index}", "{group.title()}" }
                        div { class: "catalog-picker__grid",
                            for item in group.items {
                                button {
                                    key: "{item.id.unwrap_or(0)}",
                                    class: "catalog-chip",
                                    r#type: "button",
                                    aria_label: chip_label(&item),
                                    onclick: {
                                        let item = item.clone();
                                        move |_| {
                                            added += 1;
                                            on_pick.call(item.clone());
                                        }
                                    },
                                    span { class: "catalog-chip__name", "{item.name}" }
                                    span { class: "catalog-chip__price", "{item_price_detail(&item)}" }
                                }
                            }
                        }
                    }
                }
            }
            if added() > 0 {
                p { class: "catalog-picker__count", role: "status", aria_live: "polite",
                    "{added_label(added())}"
                }
            }
            Button {
                label: "Saisie libre".to_string(),
                variant: ButtonVariant::Tonal,
                onclick: move |event| {
                    added.set(0);
                    on_free_form.call(event);
                },
            }
            Button {
                label: "Terminé".to_string(),
                variant: ButtonVariant::Filled,
                onclick: move |_| close(),
            }
        }
    }
}

/// Price as the Catalogue screen writes it — with the unit when the item has
/// one. A traiteur sells to the piece: the unit is what makes the price read.
pub fn item_price_detail(item: &CatalogItem) -> String {
    let price = format_eur(item.unit_price_cents);
    match item.unit.as_deref().map(str::trim) {
        Some(unit) if !unit.is_empty() => format!("{price} / {unit}"),
        _ => price,
    }
}

fn added_label(count: usize) -> String {
    if count > 1 {
        format!("{count} prestations ajoutées")
    } else {
        format!("{count} prestation ajoutée")
    }
}

fn chip_label(item: &CatalogItem) -> String {
    format!("Ajouter {} ({})", item.name, item_price_detail(item))
}

#[cfg(test)]
mod tests {
    use super::{added_label, chip_label, group_catalog_items, line_from_catalog_item};
    use crate::domain::models::{CatalogItem, LineInput};

    fn item(id: i64, name: &str, group_name: Option<&str>, unit_price_cents: i64) -> CatalogItem {
        CatalogItem {
            id: Some(id),
            name: name.to_string(),
            group_name: group_name.map(str::to_string),
            unit_price_cents,
            unit: Some("pièce".to_string()),
            active: true,
        }
    }

    #[test]
    fn group_catalog_items_runs_consecutive_equal_groups_together() {
        let groups = group_catalog_items(vec![
            item(1, "Café", None, 150),
            item(2, "Mini Burgers", Some("Salé"), 85),
            item(3, "Mini Wraps", Some("Salé"), 80),
            item(4, "Pièce montée 60 choux", Some("Sucré"), 45_000),
        ]);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].title(), "Autres");
        assert_eq!(groups[1].title(), "Salé");
        assert_eq!(
            groups[1]
                .items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["Mini Burgers", "Mini Wraps"]
        );
        assert_eq!(groups[2].title(), "Sucré");
    }

    #[test]
    fn group_catalog_items_splits_repeated_group_names() {
        // Run-length only: the db ordering is what keeps a group contiguous.
        let groups = group_catalog_items(vec![
            item(1, "A", Some("Salé"), 85),
            item(2, "B", None, 100),
            item(3, "C", Some("Salé"), 80),
        ]);

        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn line_from_catalog_item_copies_the_item_with_quantity_one() {
        let line = line_from_catalog_item(&item(7, "Pièce montée 60 choux", Some("Sucré"), 45_000));

        assert_eq!(
            line,
            LineInput {
                group: Some("Sucré".to_string()),
                description: "Pièce montée 60 choux".to_string(),
                quantity: 1,
                unit_price_cents: 45_000,
            }
        );
    }

    #[test]
    fn the_chip_carries_the_unit_like_the_catalogue_row_does() {
        // The two used to format the same item differently: the Catalogue
        // screen showed « 0,85 € / pièce », the chip only « 0,85 € ». For a
        // traiteur selling to the piece, the unit is what makes a price read.
        assert_eq!(
            chip_label(&item(1, "Mini Burgers", Some("Salé"), 85)),
            "Ajouter Mini Burgers (0,85 € / pièce)"
        );

        let without_unit = CatalogItem {
            unit: None,
            ..item(2, "Café", None, 150)
        };
        assert_eq!(chip_label(&without_unit), "Ajouter Café (1,50 €)");
    }

    #[test]
    fn the_added_count_agrees_in_number() {
        assert_eq!(added_label(1), "1 prestation ajoutée");
        assert_eq!(added_label(3), "3 prestations ajoutées");
    }
}
