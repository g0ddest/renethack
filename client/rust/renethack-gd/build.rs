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

#[path = "src/source_stamp.rs"]
mod source_stamp;

const ROOT_CRATE: &str = "renethack-gd";
const LICENSE_PREFIXES: [&str; 5] = ["license", "licence", "copying", "notice", "copyright"];
/// What other build scripts build in, by folder (relative to the
/// workspace) and ending: nh-i18n's every translation file.
const BUILT_IN_DIRS: [(&str, &str); 1] = [("../i18n/ru", ".toml")];

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
    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let meta = metadata(&manifest_dir, steam);
    let list = meta
        .as_ref()
        .map_err(Clone::clone)
        .and_then(rust_crates)
        .unwrap_or_else(|e| {
            println!("cargo:warning=the Rust crates' licences are not listed: {e}");
            json!({ "crates": [], "texts": [], "error": e })
        });
    std::fs::write(out.join("rust_crates.json"), list.to_string()).expect("OUT_DIR is writable");
    // a debug library knows what it is built from (src/freshness.rs); a
    // release build (every package) has none of it
    if std::env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() {
        let workspace = manifest_dir.join("..");
        let sources = meta
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|m| source_files(m, &workspace))
            .unwrap_or_else(|e| {
                println!("cargo:warning=the library's sources are not stamped: {e}");
                (Vec::new(), Vec::new())
            });
        stamp_sources(&workspace, &sources.0, &sources.1, &out);
    }
}

/// The workspace's metadata as cargo resolves it for this build.
fn metadata(manifest_dir: &Path, steam: bool) -> Result<Value, String> {
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
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

/// The crates linked into the extension and their licences.
fn rust_crates(meta: &Value) -> Result<Value, String> {
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

/// What a debug library is built from, relative to the workspace (`/`, and
/// `..` for what lies beside it): the workspace's own crates it links
/// (Cargo.toml, build.rs and every file under src/), the workspace's
/// Cargo.toml and Cargo.lock, the files their sources build in by a path
/// (include_str!, include_bytes!) and what other build scripts build in.
/// With the crates' src/ folders, for a file added there.
fn source_files(meta: &Value, workspace: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|e| format!("the workspace: {e}"))?;
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("no members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let by_name: BTreeMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or("no packages")?
        .iter()
        .filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id)))
        .filter_map(|p| Some((p["name"].as_str()?, p)))
        .collect();
    // renethack-gd and the workspace crates it depends on, for itself
    let mut crates = BTreeSet::new();
    let mut stack = vec![ROOT_CRATE];
    while let Some(name) = stack.pop() {
        let Some(p) = by_name.get(name) else {
            continue;
        };
        if !crates.insert(name) {
            continue;
        }
        for d in p["dependencies"].as_array().into_iter().flatten() {
            if d["kind"].is_null() && d["path"].is_string() {
                stack.extend(d["name"].as_str());
            }
        }
    }
    let mut files = BTreeSet::from(["Cargo.toml".to_string(), "Cargo.lock".to_string()]);
    let mut dirs = Vec::new();
    for name in crates {
        let manifest = Path::new(
            by_name[name]["manifest_path"]
                .as_str()
                .ok_or("no manifest path")?,
        );
        let dir = manifest
            .parent()
            .and_then(|d| d.canonicalize().ok())
            .ok_or_else(|| format!("{name}: no folder"))?;
        let rel = dir
            .strip_prefix(&workspace)
            .map_err(|_| format!("{name} lies outside the workspace"))?
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(format!("{rel}/Cargo.toml"));
        if dir.join("build.rs").is_file() {
            files.insert(format!("{rel}/build.rs"));
        }
        walk(&dir.join("src"), &format!("{rel}/src"), &mut files);
        dirs.push(format!("{rel}/src"));
    }
    let sources: Vec<String> = files
        .iter()
        .filter(|f| f.ends_with(".rs"))
        .cloned()
        .collect();
    for f in sources {
        let text = std::fs::read_to_string(workspace.join(&f)).unwrap_or_default();
        let dir = f.rsplit_once('/').map_or("", |(d, _)| d);
        for mac in ["include_str!(\"", "include_bytes!(\""] {
            for (at, _) in text.match_indices(mac) {
                let rest = &text[at + mac.len()..];
                let Some(end) = rest.find('"') else {
                    continue;
                };
                let path = normal(&format!("{dir}/{}", &rest[..end]));
                if workspace.join(&path).is_file() {
                    files.insert(path);
                }
            }
        }
    }
    for (dir, ending) in BUILT_IN_DIRS {
        for e in std::fs::read_dir(workspace.join(dir))
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(ending) && e.path().is_file() {
                files.insert(format!("{dir}/{name}"));
            }
        }
    }
    Ok((files.into_iter().collect(), dirs))
}

/// Every file under `dir` (as `rel`), not the hidden ones (.DS_Store).
fn walk(dir: &Path, rel: &str, files: &mut BTreeSet<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            walk(&path, &format!("{rel}/{name}"), files);
        } else if path.is_file() {
            files.insert(format!("{rel}/{name}"));
        }
    }
}

/// `a/b/../c` as `a/c`, by the words alone (a leading `..` stays).
fn normal(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

/// Each file's stamp and time beside its path (`$OUT_DIR/sources.txt`, read
/// by src/freshness.rs), and every one of them a rerun-if-changed, with the
/// src/ folders: the list can never be older than the files. Written only
/// when it changes.
fn stamp_sources(workspace: &Path, files: &[String], dirs: &[String], out: &Path) {
    let mut list = String::new();
    for f in files {
        let path = workspace.join(f);
        println!("cargo:rerun-if-changed={}", path.display());
        list.push_str(&format!(
            "{:016x} {:x} {f}\n",
            source_stamp::file_stamp(&path),
            source_stamp::written(&path)
        ));
    }
    for d in dirs {
        println!("cargo:rerun-if-changed={}", workspace.join(d).display());
    }
    let dest = out.join("sources.txt");
    if std::fs::read_to_string(&dest).ok().as_deref() != Some(list.as_str()) {
        std::fs::write(&dest, list).expect("OUT_DIR is writable");
    }
}
