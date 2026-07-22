//! Dioxus RSX screens and components. Orchestration only — business logic
//! lives in `domain`.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::domain::db::open_database;
use crate::platform::{export::generate_reference_export, paths::database_path};

const FONT_LICENSE: &str = include_str!("../../assets/fonts/LICENSE");

enum ExportStatus {
    Ready,
    Running,
    Finished(String),
}

pub fn app() -> Element {
    let database = use_context_provider(|| {
        let path = database_path().map_err(|error| error.to_string())?;
        let connection = open_database(&path).map_err(|error| {
            eprintln!("Database initialization failed: {error}");
            "Impossible d'ouvrir la base locale.".to_string()
        })?;
        Ok::<_, String>(Arc::new(connection))
    });
    let database_error = database.as_ref().err().cloned();
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
        if let Some(error) = database_error {
            p { role: "alert", "{error}" }
        }
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

                        let status_update = std::panic::catch_unwind(
                            std::panic::AssertUnwindSafe(|| *worker_status.write() = next),
                        );
                        if status_update.is_err() {
                            eprintln!("Reference PDF status target is no longer mounted");
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
