//! Share sheet (ARCHI §4 « Partage »): the PDF/PNG choice handed to the
//! system share sheet — no per-channel button, Android routes to
//! WhatsApp/email/SMS (CONTEXT.md). The options carry the real file names
//! (« devis-10.pdf ») so the gérante knows exactly what will be sent.

use dioxus::prelude::*;

use super::{
    actions::{Button, ButtonVariant},
    feedback::BottomSheet,
};

/// Format picked in the share sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareFormat {
    Pdf,
    Png,
}

/// State of a share job, driven from a worker thread.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SharePhase {
    #[default]
    Idle,
    Running,
    /// Persistent block on the screen (DESIGN.md §6), cleared by the next try.
    Failed(String),
}

/// Sheet visibility and job phase in one signal, so the worker's closing
/// write is atomic — no torn frame between the phase flip and the sheet
/// closing. `SyncStorage`: the worker thread publishes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShareState {
    pub phase: SharePhase,
    pub open: bool,
}

#[component]
pub fn ShareSheet(
    state: Signal<ShareState, SyncStorage>,
    pdf_name: String,
    png_name: String,
    on_pick: EventHandler<ShareFormat>,
) -> Element {
    let (open, loading) = {
        let state = state.read();
        (state.open, state.phase == SharePhase::Running)
    };

    rsx! {
        BottomSheet {
            id: "share-sheet".to_string(),
            // PRODUCT.md keeps the three deliveries at parity, and printing was
            // the one the app never named — it is reached from here, through the
            // Android chooser, so this is where it has to be written.
            title: "Partager ou imprimer".to_string(),
            open,
            loading,
            on_dismiss: move |_| state.write().open = false,
            p { class: "share-sheet__hint",
                "Choisissez le fichier, puis l’application — messagerie, WhatsApp ou impression."
            }
            Button {
                label: pdf_name,
                variant: ButtonVariant::Tonal,
                onclick: move |_| on_pick.call(ShareFormat::Pdf),
            }
            Button {
                label: png_name,
                variant: ButtonVariant::Tonal,
                onclick: move |_| on_pick.call(ShareFormat::Png),
            }
        }
    }
}
