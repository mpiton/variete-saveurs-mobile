//! Only module allowed to touch Android/JNI and the network: PDF/PNG export,
//! share sheet, app paths, Brevo mail client.

pub mod export;
pub mod paths;
pub mod pdf_renderer;
pub mod png_stack;
pub mod share;
