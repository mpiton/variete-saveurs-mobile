//! Catalog management screen: every item (active or not) grouped by
//! `group_name`, edited in a bottom sheet. Items are never deleted — only
//! deactivated — so issued documents always keep their copied lines.

use dioxus::prelude::*;

use crate::domain::{
    db::{list_catalog, upsert_catalog_item},
    models::CatalogItem,
    money::{format_eur, parse_eur_to_cents},
    validation::{MAX_UNIT_PRICE_CENTS, validate_catalog_items},
};

use super::{
    app::DatabaseContext,
    components::{
        BottomSheet, Button, ButtonVariant, EmptyState, ErrorBlock, OutlinedField, SegmentedButton,
        group_catalog_items,
    },
};

/// Editor draft for one catalog item, owned by the screen: raw field
/// strings. The price is typed in euros and converted to cents by the
/// domain (`money::parse_eur_to_cents`) when the sheet is saved — never a
/// float.
#[derive(Clone, Debug, Default, PartialEq)]
struct CatalogEditorState {
    /// `None` while adding an item, `Some(id)` while editing one.
    id: Option<i64>,
    name: String,
    price: String,
    group: String,
    unit: String,
    active: bool,
    name_error: Option<String>,
    price_error: Option<String>,
    save_error: Option<String>,
}

#[component]
pub(super) fn Catalog() -> Element {
    let database = use_context::<DatabaseContext>();
    let reload = use_signal(|| 0_u64);
    let mut editor = use_signal(|| None::<CatalogEditorState>);
    let catalog = {
        let database = database.clone();
        use_memo(move || {
            // Subscribing to `reload` refreshes the list after each save.
            reload();
            if database.is_ok() {
                Some(load_catalog_items(&database))
            } else {
                None
            }
        })
    };
    let catalog = catalog.read();
    let (items, load_error) = match catalog.as_ref() {
        Some(Ok(items)) => (items.as_slice(), None),
        Some(Err(error)) => (&[][..], Some(error.as_str())),
        None => (&[][..], None),
    };
    let groups = group_catalog_items(items.to_vec());

    rsx! {
        section { class: "screen catalog-screen", aria_label: "Catalogue",
            if let Some(error) = load_error {
                ErrorBlock {
                    title: "Chargement impossible".to_string(),
                    message: error.to_string(),
                }
            } else if groups.is_empty() {
                EmptyState {
                    message: "Aucun article au catalogue.".to_string(),
                    action_label: "Ajouter un article".to_string(),
                    onclick: move |_| open_new_editor(editor),
                }
            } else {
                div { class: "catalog-groups",
                    for (group_index, group) in groups.into_iter().enumerate() {
                        section {
                            key: "{group_index}",
                            class: "catalog-group",
                            aria_labelledby: "catalog-group-{group_index}",
                            h3 { id: "catalog-group-{group_index}", "{group.title()}" }
                            ul { class: "line-list",
                                for item in group.items {
                                    li { key: "{item.id.unwrap_or(0)}",
                                        button {
                                            class: "line-row__main",
                                            r#type: "button",
                                            aria_label: catalog_row_label(&item),
                                            onclick: {
                                                let item = item.clone();
                                                move |_| editor.set(Some(editor_from_item(&item)))
                                            },
                                            span { class: "line-row__description", "{item.name}" }
                                            span { class: "line-row__detail", "{item_price_detail(&item)}" }
                                            if !item.active {
                                                span { class: "catalog-row__status", "Désactivé" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Button {
                    label: "Ajouter un article".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| open_new_editor(editor),
                }
            }
            CatalogSheet {
                editor,
                on_save: move |_| save_item(&database, editor, reload),
            }
        }
    }
}

#[component]
fn CatalogSheet(
    editor: Signal<Option<CatalogEditorState>>,
    on_save: EventHandler<MouseEvent>,
) -> Element {
    let Some(state) = editor.read().clone() else {
        return rsx! {};
    };
    let title = if state.id.is_some() {
        "Modifier l’article"
    } else {
        "Nouvel article"
    };

    rsx! {
        BottomSheet {
            id: "catalog-sheet".to_string(),
            title: title.to_string(),
            open: true,
            on_dismiss: move |_| editor.set(None),
            OutlinedField {
                label: "Nom".to_string(),
                name: "catalog-name".to_string(),
                value: state.name,
                error: state.name_error,
                oninput: move |event: FormEvent| update_editor(editor, |state| {
                    state.name = event.value();
                    state.name_error = None;
                }),
            }
            div { class: "line-sheet__row",
                OutlinedField {
                    label: "Prix unitaire".to_string(),
                    name: "catalog-price".to_string(),
                    input_mode: "decimal".to_string(),
                    placeholder: "0,00".to_string(),
                    value: state.price,
                    error: state.price_error,
                    oninput: move |event: FormEvent| update_editor(editor, |state| {
                        state.price = event.value();
                        state.price_error = None;
                    }),
                }
                OutlinedField {
                    label: "Unité (optionnel)".to_string(),
                    name: "catalog-unit".to_string(),
                    placeholder: "pièce".to_string(),
                    value: state.unit,
                    oninput: move |event: FormEvent| update_editor(editor, |state| {
                        state.unit = event.value();
                    }),
                }
            }
            OutlinedField {
                label: "Groupe (optionnel)".to_string(),
                name: "catalog-group".to_string(),
                placeholder: "Salé, Sucré…".to_string(),
                value: state.group,
                oninput: move |event: FormEvent| update_editor(editor, |state| {
                    state.group = event.value();
                }),
            }
            SegmentedButton {
                label: "Visibilité dans le formulaire".to_string(),
                options: vec!["Actif".to_string(), "Désactivé".to_string()],
                selected: if state.active { 0 } else { 1 },
                on_select: move |index| update_editor(editor, |state| state.active = index == 0),
            }
            if let Some(error) = state.save_error {
                ErrorBlock {
                    title: "Enregistrement impossible".to_string(),
                    message: error,
                }
            }
            Button {
                label: "Enregistrer".to_string(),
                onclick: move |event| on_save.call(event),
            }
        }
    }
}

fn update_editor(
    mut editor: Signal<Option<CatalogEditorState>>,
    mutate: impl FnOnce(&mut CatalogEditorState),
) {
    if let Some(state) = editor.write().as_mut() {
        mutate(state);
    }
}

fn open_new_editor(mut editor: Signal<Option<CatalogEditorState>>) {
    editor.set(Some(CatalogEditorState {
        active: true,
        ..CatalogEditorState::default()
    }));
}

fn editor_from_item(item: &CatalogItem) -> CatalogEditorState {
    CatalogEditorState {
        id: item.id,
        name: item.name.clone(),
        price: cents_to_euro_input(item.unit_price_cents),
        group: item.group_name.clone().unwrap_or_default(),
        unit: item.unit.clone().unwrap_or_default(),
        active: item.active,
        ..CatalogEditorState::default()
    }
}

fn save_item(
    database: &DatabaseContext,
    mut editor: Signal<Option<CatalogEditorState>>,
    mut reload: Signal<u64>,
) {
    let Some(mut state) = editor.read().clone() else {
        return;
    };
    let Some(item) = item_from_editor(&mut state) else {
        editor.set(Some(state));
        return;
    };
    // Domain gate before writing, mirroring the desktop save command; the
    // field-level checks above should already have caught everything.
    if let Err(errors) = validate_catalog_items(std::slice::from_ref(&item)) {
        state.save_error = Some(errors.join("\n"));
        editor.set(Some(state));
        return;
    }
    match persist_item(database, &item) {
        Ok(()) => {
            editor.set(None);
            *reload.write() += 1;
        }
        Err(error) => {
            state.save_error = Some(error);
            editor.set(Some(state));
        }
    }
}

/// Commits the sheet draft to an item, annotating the draft with per-field
/// errors when a field is unusable. Bounds mirror the domain validation
/// limits so mistakes are flagged here instead of at save time.
fn item_from_editor(state: &mut CatalogEditorState) -> Option<CatalogItem> {
    let name = state.name.trim();
    state.name_error = if name.is_empty() {
        Some("Le nom est obligatoire.".to_string())
    } else {
        None
    };
    let unit_price_cents = parse_eur_to_cents(&state.price);
    state.price_error = match unit_price_cents {
        Some(cents) if cents > MAX_UNIT_PRICE_CENTS => {
            Some("Le prix dépasse la limite autorisée.".to_string())
        }
        Some(_) => None,
        None => Some("Saisir un prix au format 12,34.".to_string()),
    };
    if state.name_error.is_some() || state.price_error.is_some() {
        return None;
    }
    Some(CatalogItem {
        id: state.id,
        name: name.to_string(),
        group_name: optional_text(&state.group),
        unit_price_cents: unit_price_cents?,
        unit: optional_text(&state.unit),
        active: state.active,
    })
}

fn load_catalog_items(database: &DatabaseContext) -> Result<Vec<CatalogItem>, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    list_catalog(&connection).map_err(|error| {
        eprintln!("Catalog query failed: {error}");
        "Impossible de charger le catalogue.".to_string()
    })
}

fn persist_item(database: &DatabaseContext, item: &CatalogItem) -> Result<(), String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    upsert_catalog_item(&connection, item).map_err(|error| {
        eprintln!("Catalog save failed: {error}");
        "Impossible d’enregistrer l’article.".to_string()
    })?;
    Ok(())
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Euro prefill for the sheet, parseable by `money::parse_eur_to_cents`
/// (no thousands separator).
fn cents_to_euro_input(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{},{:02}", abs / 100, abs % 100)
}

fn item_price_detail(item: &CatalogItem) -> String {
    let price = format_eur(item.unit_price_cents);
    match item.unit.as_deref().map(str::trim) {
        Some(unit) if !unit.is_empty() => format!("{price} / {unit}"),
        _ => price,
    }
}

fn catalog_row_label(item: &CatalogItem) -> String {
    let mut label = format!("{}, {}", item.name, item_price_detail(item));
    if !item.active {
        label.push_str(", désactivé");
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogEditorState, catalog_row_label, cents_to_euro_input, editor_from_item,
        item_from_editor, item_price_detail, optional_text,
    };
    use crate::domain::{
        models::CatalogItem, money::parse_eur_to_cents, validation::MAX_UNIT_PRICE_CENTS,
    };

    #[test]
    fn item_from_editor_builds_an_item_from_valid_fields() {
        let mut state = CatalogEditorState {
            id: Some(7),
            name: " Pièce montée 60 choux ".to_string(),
            price: "450,00".to_string(),
            group: " Sucré ".to_string(),
            unit: "pièce".to_string(),
            active: true,
            ..CatalogEditorState::default()
        };

        assert_eq!(
            item_from_editor(&mut state),
            Some(CatalogItem {
                id: Some(7),
                name: "Pièce montée 60 choux".to_string(),
                group_name: Some("Sucré".to_string()),
                unit_price_cents: 45_000,
                unit: Some("pièce".to_string()),
                active: true,
            })
        );
        assert_eq!(state.name_error, None);
        assert_eq!(state.price_error, None);
    }

    #[test]
    fn item_from_editor_drops_blank_optional_fields() {
        let mut state = CatalogEditorState {
            name: "Café".to_string(),
            price: "1,50".to_string(),
            ..CatalogEditorState::default()
        };

        assert_eq!(
            item_from_editor(&mut state),
            Some(CatalogItem {
                id: None,
                name: "Café".to_string(),
                group_name: None,
                unit_price_cents: 150,
                unit: None,
                active: false,
            })
        );
    }

    #[test]
    fn item_from_editor_annotates_invalid_fields_without_losing_input() {
        let mut state = CatalogEditorState {
            id: Some(3),
            name: "   ".to_string(),
            price: "douze".to_string(),
            ..CatalogEditorState::default()
        };

        assert_eq!(item_from_editor(&mut state), None);
        assert_eq!(
            state.name_error,
            Some("Le nom est obligatoire.".to_string())
        );
        assert_eq!(
            state.price_error,
            Some("Saisir un prix au format 12,34.".to_string())
        );
        assert_eq!(state.id, Some(3));
        assert_eq!(state.name, "   ");
        assert_eq!(state.price, "douze");
    }

    #[test]
    fn item_from_editor_rejects_a_price_above_the_domain_cap() {
        let mut state = CatalogEditorState {
            name: "Article".to_string(),
            price: cents_to_euro_input(MAX_UNIT_PRICE_CENTS + 100),
            ..CatalogEditorState::default()
        };

        assert_eq!(item_from_editor(&mut state), None);
        assert_eq!(
            state.price_error,
            Some("Le prix dépasse la limite autorisée.".to_string())
        );
    }

    #[test]
    fn editor_from_item_prefills_a_parseable_price() {
        let state = editor_from_item(&CatalogItem {
            id: Some(4),
            name: "Mini Burgers".to_string(),
            group_name: Some("Salé".to_string()),
            unit_price_cents: 85,
            unit: Some("pièce".to_string()),
            active: false,
        });

        assert_eq!(state.id, Some(4));
        assert_eq!(state.price, "0,85");
        assert_eq!(parse_eur_to_cents(&state.price), Some(85));
        assert_eq!(state.group, "Salé");
        assert_eq!(state.unit, "pièce");
        assert!(!state.active);
    }

    #[test]
    fn item_price_detail_appends_the_unit_when_present() {
        let item = CatalogItem {
            id: Some(1),
            name: "Mini Burgers".to_string(),
            group_name: None,
            unit_price_cents: 85,
            unit: Some("pièce".to_string()),
            active: true,
        };
        assert_eq!(item_price_detail(&item), "0,85 € / pièce");

        let without_unit = CatalogItem {
            unit: Some("  ".to_string()),
            ..item
        };
        assert_eq!(item_price_detail(&without_unit), "0,85 €");

        let no_unit = CatalogItem {
            unit: None,
            ..without_unit
        };
        assert_eq!(item_price_detail(&no_unit), "0,85 €");
    }

    #[test]
    fn catalog_row_label_marks_inactive_items() {
        let item = CatalogItem {
            id: Some(1),
            name: "Mini Burgers".to_string(),
            group_name: None,
            unit_price_cents: 85,
            unit: Some("pièce".to_string()),
            active: true,
        };
        assert_eq!(catalog_row_label(&item), "Mini Burgers, 0,85 € / pièce");

        let inactive = CatalogItem {
            active: false,
            ..item
        };
        assert_eq!(
            catalog_row_label(&inactive),
            "Mini Burgers, 0,85 € / pièce, désactivé"
        );
    }

    #[test]
    fn optional_text_trims_and_drops_empty_values() {
        assert_eq!(optional_text(""), None);
        assert_eq!(optional_text("   "), None);
        assert_eq!(optional_text(" Salé "), Some("Salé".to_string()));
    }
}
