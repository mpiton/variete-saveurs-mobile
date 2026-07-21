#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "domain APIs land before their database and UI callers in this sprint"
    )
)]
mod domain;
mod platform;
mod ui;

fn main() {
    dioxus::launch(ui::app);
}
