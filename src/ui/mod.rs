//! Dioxus RSX screens and components. Orchestration only — business logic
//! lives in `domain`.

use dioxus::prelude::*;

use crate::platform::export::generate_reference_export;

const FONT_LICENSE: &str = include_str!("../../assets/fonts/LICENSE");

enum ExportStatus {
    Ready,
    Running,
    Finished(String),
}

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
                status.set(ExportStatus::Running);
                let mut worker_status = status;
                let worker = std::thread::Builder::new()
                    .name("reference-pdf-export".to_string())
                    .spawn(move || {
                        let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            match generate_reference_export() {
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
                            }
                        }))
                        .unwrap_or_else(|_| {
                            eprintln!("Reference PDF generation panicked");
                            ExportStatus::Finished(
                                "La génération du PDF a échoué de manière inattendue.".to_string(),
                            )
                        });

                        match worker_status.try_write() {
                            Ok(mut status) => *status = next,
                            Err(error) => {
                                eprintln!("Reference PDF status update skipped: {error}");
                            }
                        }
                    });

                if let Err(error) = worker {
                    eprintln!("Reference PDF worker could not start: {error}");
                    status.set(ExportStatus::Finished(
                        "Impossible de démarrer la génération du PDF.".to_string(),
                    ));
                }
            },
            if running { "Génération…" } else { "Générer le PDF de référence" }
        }
        p { "{message}" }
        details {
            summary { "Licences des polices" }
            pre { "{FONT_LICENSE}" }
        }
    }
}
