use dioxus::prelude::*;

use crate::domain::models::DocumentKind;

use super::actions::{Button, ButtonVariant, issue_label};
use super::feedback::BottomSheet;

/// One short pulse as the number is spent.
///
/// Android answers a committed action in the hand, and this is the only act in
/// the app that cannot be taken back — the one place where feeling it land is
/// worth more than reading about it. The snackbar that follows says the same
/// thing to someone who is looking at the screen.
///
/// Behind `prefers-reduced-motion` because `navigator.vibrate` obeys no other
/// user setting: Android's « Supprimer les animations » is the nearest signal
/// the web gets to « moins de tout ça ». `?.` because the desktop `dx serve`
/// has no vibrator, and neither does a phone whose owner revoked the
/// permission.
fn acknowledge_in_the_hand() {
    let _ = document::eval(
        "if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
             navigator.vibrate?.(20);
         }",
    );
}

/// The one confirmation the app owes her. Issuing assigns the number for good
/// and freezes the document (`CONTEXT.md` — « un document émis n'est plus
/// jamais modifié »); the only way back is a duplication, which spends another
/// number and leaves an apparent hole in her sequence. Deleting a line asks
/// twice; this asked nothing.
///
/// The number is peeked, never reserved: `numbering::next_number` only reads
/// the counter, so cancelling costs nothing.
#[component]
pub fn IssueConfirmSheet(
    kind: DocumentKind,
    number: i64,
    open: bool,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        BottomSheet {
            id: "issue-confirm-sheet".to_string(),
            title: confirm_title(&kind, number),
            open,
            on_dismiss: move |_| on_cancel.call(()),
            p {
                "Le numéro sera attribué définitivement. Un document émis ne peut plus être modifié : une correction se fait par duplication, avec un nouveau numéro."
            }
            div { class: "confirmation-actions",
                Button {
                    label: "Annuler".to_string(),
                    variant: ButtonVariant::Text,
                    onclick: move |_| on_cancel.call(()),
                }
                Button {
                    label: "Émettre".to_string(),
                    onclick: move |_| {
                        acknowledge_in_the_hand();
                        on_confirm.call(());
                    },
                }
            }
        }
    }
}

/// The number belongs in the title: « Émettre ? » asks nothing useful, whereas
/// « Émettre le devis n° 10 ? » is the fact she is about to spend.
fn confirm_title(kind: &DocumentKind, number: i64) -> String {
    format!("{} n° {number} ?", issue_label(kind))
}

#[cfg(test)]
mod tests {
    use super::confirm_title;
    use crate::domain::models::DocumentKind;

    #[test]
    fn the_confirmation_names_the_number_it_is_about_to_spend() {
        assert_eq!(
            confirm_title(&DocumentKind::Quote, 10),
            "Émettre le devis n° 10 ?"
        );
        assert_eq!(
            confirm_title(&DocumentKind::Invoice, 1),
            "Émettre la facture n° 1 ?"
        );
    }
}
