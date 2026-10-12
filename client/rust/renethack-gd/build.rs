//! A Steam build finds Valve's steam_api library beside the extension: on
//! macOS its install name says so (`@loader_path`), on Linux the runpath.
//!
//! And the list of the Rust crates linked into the extension, for the
//! game's licences (`credits.rs`): the walk `tools/dist/rust_licenses.py`
//! makes for legal/ (renethack-gd's normal dependencies for the target, no
//! proc-macros, not the workspace's own crates), each crate with its
//! licence and source, then the licence files they ship, each distinct
//! text once. Written as JSON into OUT_DIR.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const ROOT_CRATE: &str = "renethack-gd";
const LICENSE_PREFIXES: [&str; 5] = ["license", "licence", "copying", "notice", "copyright"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../Cargo.toml");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    let steam = std::env::var_os("CARGO_FEATURE_STEAM").is_some();
    if steam && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN");
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let list = rust_crates(steam).unwrap_or_else(|e| {
        println!("cargo:warning=the Rust crates' licences are not listed: {e}");
        json!({ "crates": [], "texts": [], "error": e })
    });
    std::fs::write(out.join("rust_crates.json"), list.to_string()).expect("OUT_DIR is writable");
}

/// The crates linked into the extension and their licences.
fn rust_crates(steam: bool) -> Result<Value, String> {
    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("no manifest dir")?);
    let workspace = manifest_dir.join("..").join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--offline",
        "--manifest-path",
    ])
    .arg(&workspace);
    if let Ok(target) = std::env::var("TARGET") {
        cmd.args(["--filter-platform", &target]);
    }
    if steam {
        cmd.args(["--features", "steam"]);
    }
    let output = cmd.output().map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let meta: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let packages: BTreeMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or("no packages")?
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .ok_or("no resolve")?
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("no members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let root = members
        .iter()
        .copied()
        .find(|id| packages.get(id).is_some_and(|p| p["name"] == ROOT_CRATE))
        .ok_or("renethack-gd is not in the workspace")?;
    let proc_macro = |p: &Value| {
        p["targets"].as_array().is_some_and(|ts| {
            ts.iter().any(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "proc-macro"))
            })
        })
    };
    let mut linked = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !linked.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(id) else {
            continue;
        };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let Some(pkg) = dep["pkg"].as_str() else {
                continue;
            };
            let normal = dep["dep_kinds"]
                .as_array()
                .is_some_and(|ks| ks.iter().any(|k| k["kind"].is_null()));
            if normal && packages.get(pkg).is_some_and(|p| !proc_macro(p)) {
                stack.push(pkg);
            }
        }
    }
    let mut crates: Vec<&Value> = linked
        .iter()
        .filter(|id| !members.contains(*id))
        .filter_map(|id| packages.get(id).copied())
        .collect();
    crates.sort_by_key(|p| {
        (
            p["name"].as_str().unwrap_or(""),
            p["version"].as_str().unwrap_or(""),
        )
    });

    let mut listed = Vec::new();
    // each distinct licence text once, with the crates that ship it
    let mut texts: Vec<(String, Vec<String>)> = Vec::new();
    for p in crates {
        let name = p["name"].as_str().unwrap_or("?");
        let version = p["version"].as_str().unwrap_or("?");
        let label = format!("{name} {version}");
        let license = p["license"]
            .as_str()
            .or(p["license_file"].as_str())
            .unwrap_or("?");
        let source = p["repository"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://crates.io/crates/{name}"));
        listed.push(
            json!({ "name": name, "version": version, "license": license, "source": source }),
        );
        let dir = p["manifest_path"]
            .as_str()
            .and_then(|m| Path::new(m).parent().map(Path::to_path_buf));
        let mut files: Vec<PathBuf> = dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|f| {
                f.is_file()
                    && f.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        let n = n.to_lowercase();
                        LICENSE_PREFIXES.iter().any(|p| n.starts_with(p))
                    })
            })
            .collect();
        files.sort();
        if files.is_empty() {
            let text = format!("(no licence file in the crate; licence: {license})");
            add_text(&mut texts, text, &label);
        }
        for f in files {
            let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let text = String::from_utf8_lossy(&bytes).trim().to_string();
            add_text(&mut texts, text, &label);
        }
    }
    let texts: Vec<Value> = texts
        .into_iter()
        .map(|(text, users)| json!({ "text": text, "users": users }))
        .collect();
    Ok(json!({ "crates": listed, "texts": texts, "error": null }))
}

fn add_text(texts: &mut Vec<(String, Vec<String>)>, text: String, user: &str) {
    match texts.iter_mut().find(|(t, _)| *t == text) {
        Some((_, users)) => users.push(user.to_string()),
        None => texts.push((text, vec![user.to_string()])),
    }
}
