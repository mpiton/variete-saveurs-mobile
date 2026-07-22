//! Only module allowed to touch Android/JNI and the network: PDF/PNG export,
//! share sheet, app paths, Brevo mail client.

#[expect(
    dead_code,
    reason = "the reference export API is staged for its production UI caller"
)]
pub mod export;
pub mod paths;
