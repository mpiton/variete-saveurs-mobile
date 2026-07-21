//! Dioxus RSX screens and components. Orchestration only — business logic
//! lives in `domain`.

use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;

use crate::platform::export::generate_reference_export;

enum ExportStatus {
    Ready,
    Running,
    Finished(String),
}

static EXPORT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn app() -> Element {
    let mut status = use_signal_sync(|| ExportStatus::Ready);
    let (running, message) = match &*status.read() {
        ExportStatus::Ready => (
            false,
            "Prêt à générer le document de référence.".to_string(),
        ),
        ExportStatus::Running => (true, "Génération du PDF en cours…".to_string()),
        ExportStatus::Finished(message) => (false, message.clone()),
    };

    rsx! {
        h1 { "Devis & Factures" }
        button {
            disabled: running,
            onclick: move |_| {
                if EXPORT_IN_PROGRESS.swap(true, Ordering::SeqCst) {
                    return;
                }
                status.set(ExportStatus::Running);
                let mut worker_status = status;
                std::thread::spawn(move || {
                    let next = match generate_reference_export() {
                        Ok(export) => ExportStatus::Finished(format!(
                            "PDF de {} pages généré en {} ms : {} (HTML : {})",
                            export.pages,
                            export.elapsed.as_millis(),
                            export.pdf_path.display(),
                            export.html_path.display(),
                        )),
                        Err(error) => {
                            eprintln!("Reference PDF generation failed: {error:?}");
                            ExportStatus::Finished(error.to_string())
                        }
                    };
                    EXPORT_IN_PROGRESS.store(false, Ordering::SeqCst);
                    worker_status.set(next);
                });
            },
            if running { "Génération…" } else { "Générer le PDF de référence" }
        }
        p { "{message}" }
    }
}
