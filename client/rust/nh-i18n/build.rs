//! Builds in every translation file: `client/i18n/ru/*.toml`, listed in
//! `$OUT_DIR/ru_files.rs` for `data.rs` (a file added there needs no line
//! of code).

use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/ru");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".toml"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut out = String::from("&[\n");
    for name in &names {
        let path = dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("]\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("ru_files.rs");
    std::fs::write(dest, out).expect("ru_files.rs");
}
