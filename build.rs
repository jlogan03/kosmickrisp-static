use std::{env, path::PathBuf};

const ARCHIVE: &str = "kosmickrisp-26.3.0-aarch64-apple-darwin-macos26.tar.gz";
const ARCHIVE_SHA256: &str = "d68b599425b5b76f2ff57bef812bae8c26e4dcd651c7026a24278b291c97c648";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for name in [
        "KOSMICKRISP_STATIC_DIR",
        "KOSMICKRISP_ARTIFACT_URL",
        "KOSMICKRISP_ARTIFACT_SHA256",
        "DEVELOPER_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let target = env::var("TARGET").unwrap();
    if target != "aarch64-apple-darwin" {
        // Permit workspace checks on other platforms without downloading native code.
        println!(
            "cargo:warning=kosmickrisp-static has no API on {target}; only aarch64-apple-darwin is supported"
        );
        return;
    }
    let source = env::var_os("CARGO_FEATURE_BUILD_KOSMICKRISP").is_some();
    if !source && env::var_os("KOSMICKRISP_STATIC_DIR").is_none() {
        match (
            env::var_os("KOSMICKRISP_ARTIFACT_URL"),
            env::var_os("KOSMICKRISP_ARTIFACT_SHA256"),
        ) {
            (None, None) => {
                let url = format!(
                    "https://github.com/jlogan03/kosmickrisp-static/releases/download/v{}/{ARCHIVE}",
                    env!("CARGO_PKG_VERSION")
                );
                // SAFETY: This build script has not started any threads.
                unsafe {
                    env::set_var("KOSMICKRISP_ARTIFACT_URL", url);
                    env::set_var("KOSMICKRISP_ARTIFACT_SHA256", ARCHIVE_SHA256);
                }
            }
            (Some(_), Some(_)) => {}
            _ => panic!("Set both KOSMICKRISP_ARTIFACT_URL and KOSMICKRISP_ARTIFACT_SHA256"),
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let lib = kosmickrisp_build::prepare(&out, source)
        .unwrap_or_else(|error| panic!("KosmicKrisp preparation failed: {error:#}"));
    println!("cargo:rustc-link-search=native={}", lib.display());
    // Mesa dispatch tables use weak references to optional entry points.
    println!("cargo:rustc-link-lib=static:+whole-archive=KosmicKrisp");
    println!("cargo:rustc-link-lib=dylib=c++");
    for framework in ["Metal", "Foundation", "QuartzCore", "CoreGraphics"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    if let Some(local) = env::var_os("KOSMICKRISP_STATIC_DIR") {
        println!("cargo:rerun-if-changed={}", PathBuf::from(local).display());
    }
}
