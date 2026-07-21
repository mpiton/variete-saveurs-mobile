# Dioxus pour l'app mobile Android

L'app mobile remplace l'app desktop Tauri 2 + React (`app/`). Plutôt que d'étendre Tauri au mobile (qui aurait réutilisé l'UI React), on écrit l'app en Dioxus : tout le code passe en Rust, la logique métier existante (`render.rs`, `db.rs`, `money.rs`, `validation.rs`, `models.rs`) s'appelle en direct sans couche IPC, et le template HTML/CSS du document est conservé à l'identique. Coût assumé : réécriture de l'UI React en RSX, et export PDF via JNI vers le WebView Android au lieu de l'écosystème de plugins Tauri.

## Considered Options

- **Tauri 2 Android** — réutilisait l'UI React et les plugins (share sheet, etc.) ; écarté au profit d'une base 100 % Rust.
- **React Native / Expo, Flutter** — réécriture totale de la logique métier et perte de la fidélité du template HTML/CSS ; écartés.
