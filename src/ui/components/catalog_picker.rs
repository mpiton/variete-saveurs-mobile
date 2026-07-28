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
    fields::OutlinedField,
    line_sheet::{parse_quantity, quantity_error},
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

/// Copies the item into a draft line at the quantity she just said. The line is
/// copied, never referenced: a later catalog edit cannot rewrite a draft line
/// nor an issued document.
pub fn line_from_catalog_item(item: &CatalogItem, quantity: i64) -> LineInput {
    LineInput {
        group: item.group_name.clone(),
        description: item.name.clone(),
        quantity,
        unit_price_cents: item.unit_price_cents,
    }
}

/// What she meant by what she typed. Empty is one — the placeholder says so —
/// because « une pièce montée » must stay two taps while « 40 » is typed
/// straight in, with no prefilled 1 to clear first. Anything else goes through
/// the line sheet's own rule, so a quantity means the same thing and fails with
/// the same French sentence wherever she says it.
fn picked_quantity(typed: &str) -> Result<i64, String> {
    if typed.trim().is_empty() {
        return Ok(1);
    }
    let parsed = parse_quantity(typed);
    if let Some(message) = quantity_error(parsed) {
        return Err(message);
    }
    // `quantity_error` returning None means `parsed` is Some; the fallback is
    // the empty case's own answer rather than a second rule.
    Ok(parsed.unwrap_or(1))
}

/// Brings the whole prompt into view, then puts the keypad up on the quantity.
///
/// Called from the prompt's own `onmounted`, not from the chip's `onclick`:
/// there the field does not exist yet, since Dioxus has not rendered it. The
/// section is scrolled rather than the field, because at 200 % system font the
/// sheet is 80 % of the screen and « Ajouter » sits below the fold — focusing
/// the field alone would leave the button she needs next off screen.
fn reveal_quantity_prompt() {
    let _ = dioxus::document::eval(
        "(() => {
             const prompt = document.querySelector('.catalog-picker__prompt');
             if (prompt) { prompt.scrollIntoView({ block: 'nearest' }); }
             const field = document.getElementById('field-catalog-quantity');
             if (field) { field.focus({ preventScroll: true }); }
         })()",
    );
}

#[component]
pub fn CatalogPicker(
    state: Signal<Option<Vec<CatalogItem>>>,
    on_pick: EventHandler<LineInput>,
    on_free_form: EventHandler<MouseEvent>,
) -> Element {
    // Picks accumulate instead of closing the sheet on each one: a traiteur
    // quote is five to ten items, and closing after every chip made each one
    // cost a reopen. The count is the receipt for taps that land behind the
    // scrim; it resets with the sheet.
    let mut added = use_signal(|| 0usize);
    // The item waiting for its quantity. Every catalogue line used to arrive at
    // one, so « 40 mini burgers » meant closing this sheet, tapping the row,
    // clearing the 1 and typing 40 — once per line, five to ten times a quote.
    // She says how many where she says which.
    let mut pending = use_signal(|| None::<CatalogItem>);
    let mut quantity = use_signal(String::new);
    let mut quantity_message = use_signal(|| None::<String>);
    let mut clear_prompt = move || {
        pending.set(None);
        quantity.set(String::new());
        quantity_message.set(None);
    };
    let mut close = move || {
        added.set(0);
        clear_prompt();
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
                                            // Tapping another chip while the
                                            // prompt is up switches item; it
                                            // does not add the previous one.
                                            quantity.set(String::new());
                                            quantity_message.set(None);
                                            pending.set(Some(item.clone()));
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
            if let Some(item) = pending() {
                section {
                    // Keyed on the item so tapping another chip remounts the
                    // prompt, which re-runs the reveal for the new one.
                    key: "{item.id.unwrap_or(0)}",
                    class: "catalog-picker__prompt",
                    aria_label: "Quantité pour {item.name}",
                    onmounted: move |_| reveal_quantity_prompt(),
                    p { class: "catalog-picker__prompt-name", "{item.name}" }
                    OutlinedField {
                        label: "Quantité".to_string(),
                        name: "catalog-quantity".to_string(),
                        // Empty means one, and the placeholder says so: one of
                        // something stays two taps, forty is typed straight in
                        // without clearing a prefilled 1 first.
                        placeholder: "1".to_string(),
                        input_mode: "numeric".to_string(),
                        value: quantity(),
                        error: quantity_message(),
                        oninput: move |event: FormEvent| {
                            quantity.set(event.value());
                            quantity_message.set(None);
                        },
                    }
                    Button {
                        label: "Ajouter".to_string(),
                        onclick: {
                            let item = item.clone();
                            move |_| match picked_quantity(&quantity()) {
                                Err(message) => quantity_message.set(Some(message)),
                                Ok(picked) => {
                                    added += 1;
                                    on_pick.call(line_from_catalog_item(&item, picked));
                                    clear_prompt();
                                }
                            }
                        },
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
                // While the prompt is up, « Ajouter » is the primary action and
                // this one steps down: two filled buttons in one sheet is two
                // answers to « what now ».
                variant: if pending().is_some() {
                    ButtonVariant::Tonal
                } else {
                    ButtonVariant::Filled
                },
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
    fn line_from_catalog_item_copies_the_item_at_the_quantity_she_said() {
        let line =
            line_from_catalog_item(&item(7, "Pièce montée 60 choux", Some("Sucré"), 45_000), 40);

        assert_eq!(
            line,
            LineInput {
                group: Some("Sucré".to_string()),
                description: "Pièce montée 60 choux".to_string(),
                quantity: 40,
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

    #[test]
    fn an_empty_quantity_is_one_and_a_bad_one_says_why() {
        use super::picked_quantity;

        assert_eq!(picked_quantity(""), Ok(1), "the placeholder's promise");
        assert_eq!(picked_quantity("   "), Ok(1));
        assert_eq!(picked_quantity("40"), Ok(40));
        assert_eq!(
            picked_quantity("0"),
            Err("La quantité doit être positive.".to_string())
        );
        assert_eq!(
            picked_quantity("12,5"),
            Err("Saisir une quantité entière (ex. 12).".to_string())
        );
        assert_eq!(
            picked_quantity("999999"),
            Err("La quantité dépasse la limite autorisée.".to_string())
        );
    }
}
