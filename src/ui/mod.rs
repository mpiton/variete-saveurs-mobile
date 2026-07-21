//! Dioxus RSX screens and components. Orchestration only — business logic
//! lives in `domain`.

use dioxus::prelude::*;

pub fn app() -> Element {
    rsx! {
        h1 { "Devis & Factures" }
    }
}
