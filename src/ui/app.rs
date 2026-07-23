use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    rc::Rc,
    sync::{Arc, Mutex},
};

use dioxus::{
    document::Document,
    history::{History, MemoryHistory},
    prelude::*,
    router::components::HistoryProvider,
};
use rusqlite::Connection;

use crate::{domain::db::open_database, platform::paths::database_path};

use super::home::Home;

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
        #[route("/apercu")]
        Preview {},
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
            Self::Preview {} => "Aperçu",
            Self::Compose {} => "Composition",
            Self::Catalog {} => "Catalogue",
            Self::Settings {} => "Réglages",
        }
    }
}

pub fn app() -> Element {
    let _database = use_context_provider(initialize_database);
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

#[component]
fn AppShell() -> Element {
    let current_route = use_route::<Route>();
    let navigator = use_navigator();
    let can_go_back = navigator.can_go_back();
    let database_error = use_context::<DatabaseContext>().as_ref().err().cloned();
    let history = use_context::<Rc<AppHistory>>();
    let mut menu_open = use_signal(|| false);

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

    rsx! {
        div { class: "app-shell",
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
fn Form() -> Element {
    rsx! {
        section { class: "screen", aria_labelledby: "form-title",
            div { class: "placeholder-panel",
                h2 { id: "form-title", "Formulaire" }
                p { "Structure du brouillon à venir." }
                label { class: "field",
                    "Nom du client"
                    input {
                        r#type: "text",
                        name: "client-name-placeholder",
                        autocomplete: "off",
                        placeholder: "Touchez pour vérifier le clavier",
                    }
                }
            }
        }
    }
}

#[component]
fn Record(id: i64) -> Element {
    let _ = id;
    rsx! { Placeholder { title: "Fiche", description: "Détail d’un document émis à venir." } }
}

#[component]
fn Preview() -> Element {
    rsx! { Placeholder { title: "Aperçu", description: "Aperçu plein écran à venir." } }
}

#[component]
fn Compose() -> Element {
    rsx! { Placeholder { title: "Composition", description: "Composition de l’envoi à venir." } }
}

#[component]
fn Catalog() -> Element {
    rsx! { Placeholder { title: "Catalogue", description: "Gestion du catalogue à venir." } }
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
    fn routes_have_stable_paths_and_french_titles() {
        let routes = [
            (Route::Home {}, "/", "Accueil"),
            (Route::Form {}, "/formulaire", "Formulaire"),
            (Route::Record { id: 42 }, "/fiche/42", "Fiche"),
            (Route::Preview {}, "/apercu", "Aperçu"),
            (Route::Compose {}, "/composition", "Composition"),
            (Route::Catalog {}, "/catalogue", "Catalogue"),
            (Route::Settings {}, "/reglages", "Réglages"),
        ];

        for (route, path, title) in routes {
            assert_eq!(route.to_string(), path);
            assert_eq!(route.title(), title);
        }
    }
}
