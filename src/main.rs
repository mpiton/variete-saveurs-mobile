mod domain;
mod platform;
mod ui;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    rsx! {
        h1 { "Devis & Factures" }
    }
}
