//! Share flow (ARCHI §4 « Partage », §5 `share_file`) shared by the fiche's
//! and the aperçu's « Partager »: a bottom sheet offers PDF or PNG, then a
//! worker re-exports whatever file is missing (`export_document` keeps
//! existing files — the standing re-export path) and hands its `content://`
//! URI to the Android share sheet. Fire-and-forget: success needs no
//! confirmation — the system chooser is the feedback — and a cancellation
//! (scrim, back, chooser dismissed) leaves no state behind.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dioxus::prelude::*;

use crate::domain::models::{DocumentInput, DocumentKind};
use crate::platform::export::{export_document, export_stem};
use crate::platform::share::share_file;

use super::components::{ShareFormat, SharePhase, ShareState};
use super::issue::write_from_worker;

/// Per-screen share flow state, created fresh on each mount — like every
/// screen signal here, it assumes the router remounts on id change rather
/// than re-running a mounted screen with new props.
#[derive(Clone, Copy)]
pub(super) struct ShareFlow(Signal<ShareState, SyncStorage>);

pub(super) fn use_share_flow() -> ShareFlow {
    ShareFlow(use_signal_sync(ShareState::default))
}

impl ShareFlow {
    /// Opens the sheet for a new pick. A live job is left alone: the sheet
    /// simply shows its spinner again until the worker closes it — resetting
    /// `Running` here would defeat the double-tap guard in `start` and let a
    /// second worker spawn (two stacked system choosers).
    pub(super) fn open_sheet(mut self) {
        let mut state = self.0.write();
        if state.phase != SharePhase::Running {
            state.phase = SharePhase::Idle;
        }
        state.open = true;
    }

    /// Message for the persistent error block (DESIGN.md §6), if the last
    /// job failed.
    pub(super) fn error(self) -> Option<String> {
        match &self.0.read().phase {
            SharePhase::Failed(message) => Some(message.clone()),
            _ => None,
        }
    }

    pub(super) fn state(self) -> Signal<ShareState, SyncStorage> {
        self.0
    }

    /// Starts the export-then-share chain from a sheet pick. The phase
    /// guards the double-tap: a second call while `Running` returns
    /// immediately (the sheet is also inert while loading).
    pub(super) fn start(mut self, input: DocumentInput, number: i64, format: ShareFormat) {
        if self.0.read().phase == SharePhase::Running {
            return;
        }
        self.0.write().phase = SharePhase::Running;
        let flow = self.0;
        let worker = std::thread::Builder::new().spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                let export = export_document(&input, number).map_err(|error| error.to_string())?;
                let path = match format {
                    ShareFormat::Pdf => &export.pdf_path,
                    ShareFormat::Png => &export.png_path,
                };
                share_file(path).map_err(|error| error.to_string())
            }));
            let next = match outcome {
                Ok(Ok(())) => SharePhase::Idle,
                Ok(Err(message)) => {
                    eprintln!("Share failed for {number}: {message}");
                    SharePhase::Failed(message)
                }
                Err(payload) => {
                    eprintln!("Share panicked for {number}: {payload:?}");
                    SharePhase::Failed(
                        "Échec inattendu du partage (détail dans les logs).".to_string(),
                    )
                }
            };
            // Whatever the outcome, our sheet leaves: the system chooser
            // takes over on success, the error block shows on failure.
            write_from_worker(flow, |state| {
                state.phase = next;
                state.open = false;
            });
        });
        // Fallible spawn: a resource-starved OS must not panic the UI
        // thread — the job goes straight to its terminal failure state.
        if let Err(error) = worker {
            eprintln!("Share worker could not start: {error}");
            write_from_worker(self.0, |state| {
                state.phase = SharePhase::Failed("Impossible de démarrer le partage.".to_string());
                state.open = false;
            });
        }
    }
}

/// File names shown in the sheet and served by the provider as
/// `DISPLAY_NAME` — what the recipient sees (« devis-10.pdf »).
pub(super) fn share_file_names(kind: &DocumentKind, number: i64) -> (String, String) {
    let stem = export_stem(kind, number);
    (format!("{stem}.pdf"), format!("{stem}.png"))
}

#[cfg(test)]
mod tests {
    use crate::domain::models::DocumentKind;

    use super::share_file_names;

    #[test]
    fn share_file_names_follow_the_export_stem() {
        assert_eq!(
            share_file_names(&DocumentKind::Quote, 10),
            ("devis-10.pdf".to_string(), "devis-10.png".to_string())
        );
        assert_eq!(
            share_file_names(&DocumentKind::Invoice, 3),
            ("facture-3.pdf".to_string(), "facture-3.png".to_string())
        );
    }
}
