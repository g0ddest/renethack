//! A Steam build finds Valve's steam_api library beside the extension: on
//! macOS its install name says so (`@loader_path`), on Linux the runpath.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let steam = std::env::var_os("CARGO_FEATURE_STEAM").is_some();
    if steam && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN");
    }
}
