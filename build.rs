use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/app_icon.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

        if target_env == "msvc" {
            let res_file = out_dir.join("app_icon.res");
            if let Some(rc_exe) = find_msvc_rc() {
                let status = Command::new(&rc_exe)
                    .args(["/nologo", "/fo"])
                    .arg(&res_file)
                    .arg("app.rc")
                    .status()
                    .unwrap_or_else(|e| panic!("failed to execute {}: {}", rc_exe.display(), e));
                if !status.success() {
                    panic!("rc.exe failed with status: {}", status);
                }
                println!("cargo:rustc-link-arg={}", res_file.display());
            } else {
                println!("cargo:warning=rc.exe not found; skipping Windows icon resource embedding");
            }
        } else {
            let res_obj = out_dir.join("app_icon.o");
            let windres = env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
            let status = Command::new(&windres)
                .args(["app.rc", "-O", "coff", "-o"])
                .arg(&res_obj)
                .status()
                .expect("failed to execute windres");

            if !status.success() {
                panic!("windres failed with status: {}", status);
            }

            println!("cargo:rustc-link-arg={}", res_obj.display());
        }
    }
}

fn find_msvc_rc() -> Option<PathBuf> {
    if let Ok(rc) = env::var("RC") {
        let p = PathBuf::from(rc);
        if p.is_file() {
            return Some(p);
        }
    }
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    };
    let mut roots = Vec::new();
    if let Ok(pf86) = env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(pf86).join("Windows Kits").join("10").join("bin"));
    }
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin"));

    for bin_root in roots {
        if let Ok(entries) = std::fs::read_dir(&bin_root) {
            let mut vers: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path().join(arch).join("rc.exe"))
                .filter(|p| p.is_file())
                .collect();
            vers.sort();
            if let Some(latest) = vers.pop() {
                return Some(latest);
            }
        }
    }
    None
}
