use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    match target_os.as_str() {
        "windows" => build_windows(),
        "linux"   => build_linux(),
        "macos"   => build_macos(),
        other     => println!("cargo:warning=Untested platform: {other}"),
    }
}

// ── Windows ──────────────────────────────────────────────────────────────────

fn build_windows() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib_dir  = manifest.join("mpv-lib");

    // mpv.lib: Windows import library stub included in the repo.
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=mpv");

    // Copy libmpv-2.dll next to the output binary.
    let out_dir    = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(3).unwrap();
    let dll_dst    = target_dir.join("libmpv-2.dll");

    if !dll_dst.exists() {
        let candidates = [
            std::env::var("MPV_DLL_DIR").ok()
                .map(|d| PathBuf::from(d).join("libmpv-2.dll")),
            Some(manifest.join("libmpv-2.dll")),
            Some(PathBuf::from(r"C:\Program Files\mpv.net\libmpv-2.dll")),
            Some(PathBuf::from(r"C:\Program Files\mpv\mpv-2.dll")),
        ];
        if let Some(src) = candidates.iter().flatten().find(|p| p.exists()) {
            std::fs::copy(src, &dll_dst)
                .unwrap_or_else(|e| panic!("failed to copy {}: {e}", src.display()));
        } else {
            println!(
                "cargo:warning=libmpv-2.dll not found. Copy it next to Cargo.toml \
                 or set MPV_DLL_DIR. Get it from: \
                 https://github.com/shinchiro/mpv-winbuild-cmake/releases"
            );
        }
    }

    // Embed app icon.
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.compile().expect("failed to compile Windows resources");
}

// ── Linux ────────────────────────────────────────────────────────────────────

fn build_linux() {
    // Use pkg-config to find libmpv (installed via distro package manager).
    // e.g. apt install libmpv-dev  /  pacman -S mpv
    if pkg_config_probe("mpv") { return; }

    // Fallback: just link by name and hope it's on the default library path.
    println!("cargo:rustc-link-lib=dylib=mpv");
    println!("cargo:warning=libmpv not found via pkg-config. Install: apt install libmpv-dev");
}

// ── macOS ────────────────────────────────────────────────────────────────────

fn build_macos() {
    // Homebrew: brew install mpv
    if let Ok(prefix) = std::process::Command::new("brew")
        .args(["--prefix", "mpv"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    {
        if !prefix.is_empty() {
            println!("cargo:rustc-link-search=native={prefix}/lib");
        }
    }
    println!("cargo:rustc-link-lib=dylib=mpv");
    if pkg_config_probe("mpv") { return; }
    println!("cargo:warning=libmpv not found. Install via: brew install mpv");
}

fn pkg_config_probe(lib: &str) -> bool {
    std::process::Command::new("pkg-config")
        .args(["--libs", "--cflags", lib])
        .output()
        .map(|o| {
            if o.status.success() {
                let flags = String::from_utf8_lossy(&o.stdout);
                for flag in flags.split_whitespace() {
                    if let Some(path) = flag.strip_prefix("-L") {
                        println!("cargo:rustc-link-search=native={path}");
                    } else if let Some(lib) = flag.strip_prefix("-l") {
                        println!("cargo:rustc-link-lib=dylib={lib}");
                    }
                }
                true
            } else { false }
        })
        .unwrap_or(false)
}
