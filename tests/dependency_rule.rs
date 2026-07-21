//! ARCHI §2 dependency rule: `domain/` stays pure — no `platform::` and no
//! `dioxus` usage. Checked here instead of by review alone.

use std::fs;
use std::path::Path;

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("read domain dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn domain_has_no_platform_or_dioxus_imports() {
    let domain = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    let mut files = Vec::new();
    rust_sources(&domain, &mut files);
    assert!(!files.is_empty(), "src/domain contains no Rust files");

    for file in files {
        let source = fs::read_to_string(&file).expect("read source file");
        for (i, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for forbidden in ["platform::", "crate::platform", "dioxus"] {
                assert!(
                    !code.contains(forbidden),
                    "{}:{} uses `{}` — domain must stay pure (ARCHI §2)",
                    file.display(),
                    i + 1,
                    forbidden
                );
            }
        }
    }
}
