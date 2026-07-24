use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
    time::Duration,
};

use dioxus::{
    document::Document,
    history::{History, MemoryHistory},
    prelude::*,
    router::components::HistoryProvider,
};
use rusqlite::Connection;
use tokio::time::sleep;

use crate::{
    domain::{db::open_database, models::Document as DomainDocument},
    platform::{export::generate_reference_export, paths::database_path, share::share_file},
};

use super::{
    catalog::Catalog,
    components::{Button, ButtonVariant, ErrorBlock, Snackbar},
    form::Form,
    home::Home,
    issue::{ExportPhase, IssueFlow, IssuePhase, dismiss_notice, reset_issue_flow, retry_export},
    preview::Preview,
};

const APP_CSS: Asset = asset!("/assets/app.css");
const PRE_RENDER_STYLE: &str =
    "html,body,#main{width:100%;height:100%;margin:0;background:#0F3F3A}";
const BACK_EVENT_BRIDGE: &str = r#"
    window.addEventListener("popstate", event => {
        if (Number.isInteger(event.state?.dioxusPosition)) {
            dioxus.send(event.state.dioxusPosition);
        }
    });
    await new Promise(() => {});
"#;

pub(super) type DatabaseContext = Result<Arc<Mutex<Connection>>, String>;

/// Bumped by the app shell on any tap or scroll gesture that bubbles up to
/// it, so screens can dismiss transient affordances on outside interaction
/// (the form's client suggestions close this way — their wrapper stops its
/// own taps from bubbling to the shell).
#[derive(Clone, Copy)]
pub(super) struct OutsideInteraction(pub Signal<u64>);

struct AppHistory {
    memory: MemoryHistory,
    document: Rc<dyn Document>,
    position: Cell<i32>,
    updater: RefCell<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl AppHistory {
    fn new(document: Rc<dyn Document>) -> Self {
        let _ = document.eval(
            "window.history.replaceState({ dioxusPosition: 0 }, '', window.location.href)"
                .to_string(),
        );
        Self {
            memory: MemoryHistory::default(),
            document,
            position: Cell::new(0),
            updater: RefCell::new(None),
        }
    }

    fn browser_moved_to(&self, position: i32) {
        if position < 0 || position == self.position.get() {
            return;
        }

        match position.cmp(&self.position.get()) {
            Ordering::Less => {
                for _ in position..self.position.get() {
                    self.memory.go_back();
                }
            }
            Ordering::Greater => {
                for _ in self.position.get()..position {
                    self.memory.go_forward();
                }
            }
            Ordering::Equal => return,
        }

        self.position.set(position);
        if let Some(update) = self.updater.borrow().as_ref() {
            update();
        }
    }

    /// Pops the current entry without telling the router: used right before a
    /// `replace`, so the replace swallows the *previous* entry as well (the
    /// issue flow turns Home → Form → Preview into Home → Record, and Back
    /// from the fiche reaches a live route). The browser `popstate` that
    /// follows resolves to the already-updated position — the bridge's early
    /// return makes it a no-op.
    fn pop_silently(&self) {
        if self.can_go_back() {
            self.memory.go_back();
            self.position.set(self.position.get() - 1);
            let _ = self.document.eval("window.history.back()".to_string());
        }
    }
}

impl History for AppHistory {
    fn current_route(&self) -> String {
        self.memory.current_route()
    }

    fn current_prefix(&self) -> Option<String> {
        self.memory.current_prefix()
    }

    fn can_go_back(&self) -> bool {
        self.memory.can_go_back()
    }

    fn go_back(&self) {
        if self.can_go_back() {
            let _ = self.document.eval("window.history.back()".to_string());
        }
    }

    fn can_go_forward(&self) -> bool {
        self.memory.can_go_forward()
    }

    fn go_forward(&self) {
        if self.can_go_forward() {
            let _ = self.document.eval("window.history.forward()".to_string());
        }
    }

    fn push(&self, route: String) {
        if self.current_route() == route {
            return;
        }

        self.memory.push(route);
        let position = self.position.get() + 1;
        self.position.set(position);
        let _ = self.document.eval(format!(
            "window.history.pushState({{ dioxusPosition: {position} }}, '', window.location.href)"
        ));
    }

    fn replace(&self, route: String) {
        self.memory.replace(route);
        let position = self.position.get();
        let _ = self.document.eval(format!(
            "window.history.replaceState({{ dioxusPosition: {position} }}, '', window.location.href)"
        ));
    }

    fn updater(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.updater.replace(Some(callback));
    }
}

#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub(super) enum Route {
    #[layout(AppShell)]
        #[route("/")]
        Home {},
        #[route("/formulaire")]
        Form {},
        #[route("/fiche/:id")]
        Record { id: i64 },
        #[route("/apercu?:document")]
        Preview { document: Option<i64> },
        #[route("/composition")]
        Compose {},
        #[route("/catalogue")]
        Catalog {},
        #[route("/reglages")]
        Settings {},
}

impl Route {
    const fn title(&self) -> &'static str {
        match self {
            Self::Home {} => "Accueil",
            Self::Form {} => "Formulaire",
            Self::Record { .. } => "Fiche",
            Self::Preview { .. } => "Aperçu",
            Self::Compose {} => "Composition",
            Self::Catalog {} => "Catalogue",
            Self::Settings {} => "Réglages",
        }
    }
}

pub fn app() -> Element {
    let _database = use_context_provider(initialize_database);
    let issue_flow = use_signal_sync(|| IssuePhase::Idle);
    use_context_provider(move || IssueFlow(issue_flow));
    let history = use_hook(|| Rc::new(AppHistory::new(document::document())));
    let history_context = history.clone();
    use_context_provider(move || history_context);

    rsx! {
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content",
        }
        document::Title { "Devis & Factures" }
        style { dangerous_inner_html: PRE_RENDER_STYLE }
        document::Stylesheet { href: APP_CSS }
        HistoryProvider {
            history: move |_| history.clone() as Rc<dyn History>,
            Router::<Route> {}
        }
    }
}

fn initialize_database() -> DatabaseContext {
    let path = database_path().map_err(|error| error.to_string())?;
    let connection = open_database(&path).map_err(|error| {
        eprintln!("Database initialization failed: {error}");
        "Impossible d'ouvrir la base locale.".to_string()
    })?;
    Ok(Arc::new(connection))
}

// Debug-only harness for the PDF fidelity checks (task 05 and later spot
// checks): runs the blocking Typst export on a worker thread so the
// single-threaded UI executor never stalls. Kept out of release builds.
enum DebugExportStatus {
    Ready,
    Running,
    Finished(String),
}

static DEBUG_EXPORT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq)]
enum FlowTarget {
    Stay,
    Form,
    Record(i64),
}

#[component]
fn AppShell() -> Element {
    let current_route = use_route::<Route>();
    let navigator = use_navigator();
    let can_go_back = navigator.can_go_back();
    let database_error = use_context::<DatabaseContext>().as_ref().err().cloned();
    let history = use_context::<Rc<AppHistory>>();
    let mut menu_open = use_signal(|| false);
    let mut outside_interaction = use_context_provider(|| OutsideInteraction(Signal::new(0_u64)));
    let mut debug_export_status = use_signal_sync(|| DebugExportStatus::Ready);

    use_future(move || {
        let history = history.clone();
        async move {
            let mut back_events = document::eval(BACK_EVENT_BRIDGE);
            while let Ok(position) = back_events.recv::<i32>().await {
                history.browser_moved_to(position);
                menu_open.set(false);
            }
        }
    });

    // Issue flow navigation: the worker thread publishes phases, this turns
    // phase TRANSITIONS into routes — issued → the fiche (replaces the stale
    // draft screen: back lands on a live route, not on « Brouillon
    // introuvable »), invalid/failed → the form (the only screen rendering
    // those errors). Later mutations inside `Issued` (export retry, notice)
    // must NOT re-navigate: the remembered target makes the effect fire only
    // on real transitions.
    let flow_navigator = navigator;
    let flow_history = use_context::<Rc<AppHistory>>();
    let route_before_flow = current_route.clone();
    let issue_flow = use_context::<IssueFlow>();
    let mut last_flow_target = use_signal(|| None::<FlowTarget>);
    use_effect(move || {
        let target = match &*issue_flow.0.read() {
            IssuePhase::Idle | IssuePhase::Running => FlowTarget::Stay,
            IssuePhase::Invalid(_) | IssuePhase::Failed(_) => FlowTarget::Form,
            IssuePhase::Issued(state) => FlowTarget::Record(state.document.id),
        };
        if last_flow_target.peek().as_ref() == Some(&target) {
            return;
        }
        last_flow_target.set(Some(target));
        match target {
            FlowTarget::Stay => {}
            FlowTarget::Form => {
                flow_navigator.push(Route::Form {});
            }
            FlowTarget::Record(id) => {
                // Issuing from the draft preview: drop the preview entry too,
                // so the replace also swallows the now-dead draft form (its
                // draft is cleared) — Back from the fiche reaches a live
                // route instead of « Brouillon introuvable ».
                if matches!(route_before_flow, Route::Preview { .. }) {
                    flow_history.pop_silently();
                }
                flow_navigator.replace(Route::Record { id });
            }
        }
    });

    let (debug_export_running, debug_export_message) = match &*debug_export_status.read() {
        DebugExportStatus::Ready => (false, None),
        DebugExportStatus::Running => (true, Some("Génération du PDF en cours…".to_string())),
        DebugExportStatus::Finished(message) => (false, Some(message.clone())),
    };

    rsx! {
        div { class: "app-shell",
            // Broadcast every tap and scroll gesture in the shell (top bar,
            // scroll gutter included) so transient affordances can dismiss
            // themselves; they stop their own inner taps from bubbling here.
            onclick: move |_| *outside_interaction.0.write() += 1,
            ontouchmove: move |_| *outside_interaction.0.write() += 1,
            onwheel: move |_| *outside_interaction.0.write() += 1,
            header { class: "top-app-bar",
                if can_go_back {
                    button {
                        class: "icon-button",
                        r#type: "button",
                        aria_label: "Revenir à l’écran précédent",
                        onclick: move |_| navigator.go_back(),
                        span { aria_hidden: "true", "←" }
                    }
                }
                h1 { class: "top-app-bar__title", "{current_route.title()}" }
                button {
                    class: "icon-button",
                    r#type: "button",
                    aria_label: "Ouvrir le menu",
                    aria_controls: "app-menu",
                    aria_expanded: menu_open(),
                    aria_haspopup: "menu",
                    onclick: move |_| menu_open.toggle(),
                    span { aria_hidden: "true", "⋮" }
                }
                if menu_open() {
                    nav { id: "app-menu", class: "app-menu", aria_label: "Navigation secondaire",
                        Link {
                            role: "menuitem",
                            to: Route::Catalog {},
                            onclick: move |_| menu_open.set(false),
                            "Catalogue"
                        }
                        Link {
                            role: "menuitem",
                            to: Route::Settings {},
                            onclick: move |_| menu_open.set(false),
                            "Réglages"
                        }
                        if cfg!(debug_assertions) {
                            button {
                                r#type: "button",
                                role: "menuitem",
                                disabled: debug_export_running,
                                onclick: move |_| {
                                    if DEBUG_EXPORT_IN_PROGRESS.swap(true, AtomicOrdering::SeqCst) {
                                        return;
                                    }
                                    debug_export_status.set(DebugExportStatus::Running);
                                    let mut worker_status = debug_export_status;
                                    std::thread::spawn(move || {
                                        let outcome =
                                            std::panic::catch_unwind(generate_reference_export);
                                        let next = match outcome {
                                            Ok(Ok(export)) => {
                                                // Best-effort: the share sheet is the
                                                // visible confirmation on device; the
                                                // export itself already succeeded.
                                                if let Err(error) = share_file(&export.pdf_path) {
                                                    eprintln!("Debug share sheet failed: {error}");
                                                }
                                                DebugExportStatus::Finished(format!(
                                                    "PDF de {} pages généré en {} ms : {} (HTML : {})",
                                                    export.pages,
                                                    export.elapsed.as_millis(),
                                                    export.pdf_path.display(),
                                                    export.html_path.display(),
                                                ))
                                            }
                                            Ok(Err(error)) => {
                                                eprintln!("Debug reference export failed: {error}");
                                                DebugExportStatus::Finished(error.to_string())
                                            }
                                            Err(payload) => {
                                                eprintln!(
                                                    "Debug reference export panicked: {payload:?}"
                                                );
                                                DebugExportStatus::Finished(
                                                    "Échec inattendu de la génération du PDF (détail dans les logs)."
                                                        .to_string(),
                                                )
                                            }
                                        };
                                        DEBUG_EXPORT_IN_PROGRESS
                                            .store(false, AtomicOrdering::SeqCst);
                                        worker_status.set(next);
                                    });
                                },
                                if debug_export_running {
                                    "Génération…"
                                } else {
                                    "Générer le PDF de référence"
                                }
                            }
                            if let Some(message) = &debug_export_message {
                                p { role: "status", aria_live: "polite", "{message}" }
                            }
                        }
                    }
                }
            }
            main { class: "screen-scroll",
                if let Some(error) = database_error {
                    p { class: "startup-error", role: "alert", "{error}" }
                }
                Outlet::<Route> {}
            }
        }
    }
}

#[component]
fn Record(id: i64) -> Element {
    let navigator = use_navigator();
    let issue_flow = use_context::<IssueFlow>();
    // Post-issue state published by the flow: the fiche confirms the emission
    // (snackbar) and carries the re-export path when the PDF failed (ARCHI §4
    // — the number is never rolled back after commit).
    let (issued_here, title, notice, export_running, export_failed) = match &*issue_flow.0.read() {
        IssuePhase::Issued(state) if state.document.id == id => (
            true,
            document_title(&state.document),
            state.notice.clone(),
            state.export == ExportPhase::Running,
            state.export == ExportPhase::Failed,
        ),
        _ => (false, "Fiche".to_string(), None, false, false),
    };

    // The snackbar is transient (DESIGN.md §6): auto-dismiss after a few
    // seconds, and the timer only ever dismisses ITS notice — a newer one
    // (retry result, newer issuance) survives an older timer.
    let notice_flow = issue_flow;
    use_effect(move || {
        let expected = match &*notice_flow.0.read() {
            IssuePhase::Issued(state) => state.notice.clone(),
            _ => None,
        };
        if let Some(expected) = expected {
            spawn(async move {
                sleep(NOTICE_DURATION).await;
                dismiss_notice(notice_flow, &expected);
            });
        }
    });

    // Leaving the fiche ends the post-emission moment: no stale snackbar or
    // retry block on later visits (the aperçu's « Exporter » stays the
    // standing re-export path).
    let reset_flow = issue_flow;
    use_drop(move || reset_issue_flow(reset_flow));

    rsx! {
        section { class: "screen",
            div { class: "placeholder-panel",
                h2 { "{title}" }
                if !issued_here {
                    p { "Détail d’un document émis à venir." }
                }
                if export_running {
                    p { role: "status", aria_live: "polite", "Génération du PDF en cours…" }
                }
                if export_failed {
                    ErrorBlock {
                        title: "PDF non généré".to_string(),
                        message: "Le document est bien émis et son numéro est conservé. Réessayez l’export.".to_string(),
                    }
                    Button {
                        label: "Réessayer l’export".to_string(),
                        variant: ButtonVariant::Tonal,
                        onclick: move |_| retry_export(issue_flow),
                    }
                }
                Button {
                    label: "Aperçu".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| {
                        navigator.push(Route::Preview { document: Some(id) });
                    },
                }
            }
            if let Some(message) = notice {
                Snackbar { message }
            }
        }
    }
}

const NOTICE_DURATION: Duration = Duration::from_secs(4);

fn document_title(document: &DomainDocument) -> String {
    format!("{} n° {}", document.input.kind.label(), document.number)
}

#[component]
fn Compose() -> Element {
    rsx! { Placeholder { title: "Composition", description: "Composition de l’envoi à venir." } }
}

#[component]
fn Settings() -> Element {
    rsx! { Placeholder { title: "Réglages", description: "Configuration de l’application à venir." } }
}

#[component]
fn Placeholder(title: &'static str, description: &'static str) -> Element {
    rsx! {
        section { class: "screen",
            div { class: "placeholder-panel",
                h2 { "{title}" }
                p { "{description}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use dioxus::{
        document::{Document, NoOpDocument},
        history::History,
    };

    use super::{AppHistory, Route};

    #[test]
    fn app_history_deduplicates_routes_and_follows_browser_position() {
        let document: Rc<dyn Document> = Rc::new(NoOpDocument);
        let history = AppHistory::new(document);

        history.push(Route::Form {}.to_string());
        history.push(Route::Form {}.to_string());
        assert_eq!(history.current_route(), Route::Form {}.to_string());
        assert_eq!(history.position.get(), 1);

        history.browser_moved_to(0);
        assert_eq!(history.current_route(), Route::Home {}.to_string());

        history.browser_moved_to(1);
        assert_eq!(history.current_route(), Route::Form {}.to_string());
    }

    #[test]
    fn pop_silently_drops_the_current_entry_so_a_replace_swallows_the_previous_one() {
        let document: Rc<dyn Document> = Rc::new(NoOpDocument);
        let history = AppHistory::new(document);

        history.push(Route::Form {}.to_string());
        history.push(Route::Preview { document: None }.to_string());
        assert_eq!(history.position.get(), 2);

        history.pop_silently();
        assert_eq!(history.position.get(), 1);
        assert_eq!(history.current_route(), Route::Form {}.to_string());

        // The popstate bridge landing on the revealed entry must be a no-op.
        history.browser_moved_to(1);
        assert_eq!(history.current_route(), Route::Form {}.to_string());

        // Home → Form → Preview becomes Home → Record: Back reaches Home.
        history.replace(Route::Record { id: 7 }.to_string());
        assert_eq!(history.current_route(), Route::Record { id: 7 }.to_string());
        history.browser_moved_to(0);
        assert_eq!(history.current_route(), Route::Home {}.to_string());
    }

    #[test]
    fn routes_have_stable_paths_and_french_titles() {
        let routes = [
            (Route::Home {}, "/", "Accueil"),
            (Route::Form {}, "/formulaire", "Formulaire"),
            (Route::Record { id: 42 }, "/fiche/42", "Fiche"),
            (Route::Preview { document: None }, "/apercu?", "Aperçu"),
            (
                Route::Preview { document: Some(42) },
                "/apercu?document=42",
                "Aperçu",
            ),
            (Route::Compose {}, "/composition", "Composition"),
            (Route::Catalog {}, "/catalogue", "Catalogue"),
            (Route::Settings {}, "/reglages", "Réglages"),
        ];

        for (route, path, title) in routes {
            assert_eq!(route.to_string(), path);
            assert_eq!(route.title(), title);
        }

        // The empty-query draft path must round-trip: the history stores the
        // serialized route and the router re-parses it on every navigation.
        assert_eq!(
            "/apercu?".parse::<Route>().ok(),
            Some(Route::Preview { document: None })
        );
        assert_eq!(
            "/apercu?document=42".parse::<Route>().ok(),
            Some(Route::Preview { document: Some(42) })
        );
    }
}
