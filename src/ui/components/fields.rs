use dioxus::prelude::*;

/// Moves the keyboard on to the next field she has to fill.
///
/// `enterkeyhint="next"` only relabels the IME's action key — nothing moves
/// unless the app moves it, and DESIGN.md §7 forbids promising a gesture that
/// does not exist. The next field is resolved in the DOM rather than declared at
/// the call site, because what is rendered is not fixed: the form hides SIRET
/// and the billing address behind the « Professionnel » segment, and the home
/// search only exists past fifteen documents. The walk is scoped to the screen
/// or the sheet the field belongs to, so a run never leaks across a scrim.
fn focus_next_field(current_id: &str) {
    let id = serde_json::to_string(current_id).expect("serializing a string cannot fail");
    let _ = document::eval(&format!(
        "(() => {{
             const field = document.getElementById({id});
             if (!field) {{ return; }}
             const scope = field.closest('.bottom-sheet, .screen');
             if (!scope) {{ return; }}
             const fields = [...scope.querySelectorAll(
                 'input:not([disabled]):not([type=radio]), textarea:not([disabled])'
             )];
             const index = fields.indexOf(field);
             const next = index < 0 ? null : fields[index + 1];
             // Last of its run: close the keyboard rather than leave the action
             // key pointing at nothing.
             if (next) {{ next.focus(); }} else {{ field.blur(); }}
         }})()"
    ));
}

#[component]
pub fn OutlinedField(
    label: String,
    name: String,
    value: String,
    oninput: EventHandler<FormEvent>,
    #[props(default)] id_suffix: Option<String>,
    #[props(default = "text".to_string())] input_type: String,
    #[props(default)] input_mode: Option<String>,
    #[props(default)] autocomplete: Option<String>,
    /// What the IME's action key says, and what it does. `"next"` also moves the
    /// focus to the following field of the same screen or sheet; any other value
    /// only relabels the key. Left unset on the last field of a run, where the
    /// keyboard's own default (close) is the right answer.
    #[props(default)]
    enter_key_hint: Option<String>,
    /// Keeps the value out of the text-assistance surfaces that `password`
    /// suppresses for free: the IME's suggestion strip and its personalised
    /// learning, and the browser's own writing suggestions. A field holding a
    /// secret has to ask for them once it can be revealed as plain `text`.
    #[props(default)]
    sensitive: bool,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: Option<String>,
    /// Whether the error message is its own live region. It is, unless the
    /// caller already has one: the brouillon publishes an aggregated block and
    /// moves the focus onto the first faulty field, so three announcements land
    /// on one tap. Everywhere else — the recipient on the compose screen, the
    /// quantity and the price in a sheet, the catalogue, the Brevo key — this
    /// message is the only thing that speaks, and a caller that sets an error
    /// without offering something better must not be able to silence it by
    /// forgetting a prop.
    #[props(default = true)]
    announce_error: bool,
    #[props(default)] onfocus: Option<EventHandler<FocusEvent>>,
) -> Element {
    let input_id = field_id(&name, id_suffix.as_deref());
    let error_id = format!("{input_id}-error");
    let has_error = error.is_some();
    let error_reference = error.as_ref().map(|_| error_id.clone());
    let chains_to_next = enter_key_hint.as_deref() == Some("next");
    let keydown_id = input_id.clone();

    rsx! {
        div {
            class: "outlined-field",
            aria_busy: loading,
            input {
                id: input_id.clone(),
                name,
                r#type: input_type,
                inputmode: input_mode,
                enterkeyhint: enter_key_hint,
                autocomplete,
                // Chromium maps these to TYPE_TEXT_FLAG_NO_SUGGESTIONS and to
                // its own suggestion UI respectively; both are inert when the
                // field is a `password`.
                spellcheck: sensitive.then_some("false"),
                "writingsuggestions": sensitive.then_some("false"),
                value,
                placeholder,
                disabled: disabled || loading,
                aria_busy: loading,
                aria_invalid: has_error,
                aria_describedby: error_reference,
                oninput: move |event| oninput.call(event),
                onkeydown: move |event: KeyboardEvent| {
                    if chains_to_next && event.key() == Key::Enter {
                        // Nothing here is inside a `<form>`, so Enter has no
                        // default worth keeping and every IME sends it.
                        event.prevent_default();
                        focus_next_field(&keydown_id);
                    }
                },
                onfocus: move |event| {
                    if let Some(handler) = &onfocus {
                        handler.call(event);
                    }
                },
            }
            label { r#for: input_id, "{label}" }
            if loading {
                span { class: "spinner outlined-field__spinner", role: "status", aria_label: "Chargement" }
            }
            if let Some(ref message) = error {
                p {
                    id: error_id,
                    class: "outlined-field__error",
                    role: announce_error.then_some("alert"),
                    "{message}"
                }
            }
        }
    }
}

fn field_id(name: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(suffix) => format!("field-{name}-{suffix}"),
        None => format!("field-{name}"),
    }
}

/// Multiline counterpart of `OutlinedField` (same floating label, error and
/// disabled styling) for long free text — the compose screen's email body
/// (task 27). No loading state: nothing async ever fills it.
#[component]
pub fn OutlinedTextArea(
    label: String,
    name: String,
    value: String,
    oninput: EventHandler<FormEvent>,
    #[props(default)] id_suffix: Option<String>,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] error: Option<String>,
    /// Same rule as the single-line field.
    #[props(default = true)]
    announce_error: bool,
) -> Element {
    let input_id = field_id(&name, id_suffix.as_deref());
    let error_id = format!("{input_id}-error");
    let has_error = error.is_some();
    let error_reference = error.as_ref().map(|_| error_id.clone());

    rsx! {
        div {
            class: "outlined-field",
            textarea {
                id: input_id.clone(),
                name,
                value,
                placeholder,
                disabled,
                aria_invalid: has_error,
                aria_describedby: error_reference,
                oninput: move |event| oninput.call(event),
            }
            label { r#for: input_id, "{label}" }
            if let Some(ref message) = error {
                p {
                    id: error_id,
                    class: "outlined-field__error",
                    role: announce_error.then_some("alert"),
                    "{message}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::field_id;

    #[test]
    fn optional_suffix_disambiguates_repeated_field_names() {
        assert_eq!(field_id("price", None), "field-price");
        assert_eq!(field_id("price", Some("line-2")), "field-price-line-2");
    }
}
