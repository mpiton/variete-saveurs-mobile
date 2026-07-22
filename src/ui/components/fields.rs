use dioxus::prelude::*;

#[component]
pub fn OutlinedField(
    label: String,
    name: String,
    value: String,
    oninput: EventHandler<FormEvent>,
    #[props(default = "text".to_string())] input_type: String,
    #[props(default)] input_mode: Option<String>,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: Option<String>,
) -> Element {
    let input_id = format!("field-{name}");
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
                value,
                placeholder,
                disabled: disabled || loading,
                aria_busy: loading,
                aria_invalid: has_error,
                aria_describedby: error_reference.clone(),
                aria_errormessage: error_reference,
                oninput: move |event| oninput.call(event),
            }
            label { r#for: input_id, "{label}" }
            if loading {
                span { class: "spinner outlined-field__spinner", aria_hidden: "true" }
            }
            if let Some(ref message) = error {
                p { id: error_id, class: "outlined-field__error", role: "alert", "{message}" }
            }
        }
    }
}
