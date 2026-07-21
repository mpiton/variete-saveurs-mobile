mod domain;
mod platform;
mod ui;

fn main() {
    dioxus::launch(ui::app);
}
