use dioxus::prelude::*;

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
    #[props(default)] onfocus: Option<EventHandler<FocusEvent>>,
) -> Element {
    let input_id = field_id(&name, id_suffix.as_deref());
    let error_id = format!("{input_id}-error");
    let has_error = error.is_some();
    let error_reference = error.as_ref().map(|_| error_id.clone());

    rsx! {
        div {
            class: "outlined-field",
            aria_busy: loading,
            input {
                id: input_id.clone(),
                name,
                r#type: input_type,
                inputmode: input_mode,
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
                p { id: error_id, class: "outlined-field__error", role: "alert", "{message}" }
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
                p { id: error_id, class: "outlined-field__error", role: "alert", "{message}" }
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
