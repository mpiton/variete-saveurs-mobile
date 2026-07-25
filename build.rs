//! Copies `android/res/` into the Gradle project that `dx` generates.
//!
//! The Dioxus build only reads a manifest and a MainActivity from the crate
//! (ARCHI §4 « Partage »), and it rewrites its own `res/` folder on every
//! build — so the launcher icon has to be injected here, between `dx`'s
//! scaffolding and Gradle.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=android/res");

    // Only set when `dx` cross-compiles for Android. It points at
    // <gradle>/app/src/main/kotlin/dev/dioxus/main.
    let Ok(kotlin_dir) = std::env::var("WRY_ANDROID_KOTLIN_FILES_OUT_DIR") else {
        return;
    };
    let res = Path::new(&kotlin_dir)
        .ancestors()
        .find(|dir| dir.ends_with("src/main"))
        .unwrap_or_else(|| panic!("unexpected kotlin output dir: {kotlin_dir}"))
        .join("res");

    copy_tree(Path::new("android/res"), &res);

    // `dx` may recreate the Gradle project without these files; make Cargo
    // notice so the build script runs again instead of leaving a broken APK.
    println!(
        "cargo:rerun-if-changed={}",
        res.join("mipmap-anydpi-v26/ic_launcher_vs.xml").display()
    );
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|e| panic!("create {}: {e}", to.display()));
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("read {}: {e}", from.display())) {
        let entry = entry.unwrap_or_else(|e| panic!("read {}: {e}", from.display()));
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target)
                .unwrap_or_else(|e| panic!("copy to {}: {e}", target.display()));
        }
    }
}
