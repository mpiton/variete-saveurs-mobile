//! Business logic migrated from the desktop app: models, money, validation,
//! render, db, numbering, convert. Pure Rust — no `dioxus::` or `platform::`
//! imports (enforced by `tests/dependency_rule.rs`).

pub mod convert;
pub mod db;
pub mod models;
pub mod money;
pub mod numbering;
pub mod render;
pub mod validation;
