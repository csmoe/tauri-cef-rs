#[cfg(not(feature = "dox"))]
fn main() -> Result<(), String> {
    println!("cargo::rerun-if-changed=build.rs");

    let path = std::env::var("FLATPAK")
        .map(|_| String::from("/usr/lib"))
        .or_else(|_| std::env::var("CEF_PATH"))
        .or_else(|_| {
            std::env::var("HOME").map(|mut val| {
                val.push_str("/.local/share/cef");
                val
            })
        })
        .map_err(|e| format!("Couldn't get the path of shared library: {e}"))?;

    let path = std::path::PathBuf::from(path).canonicalize().unwrap();
    let path = path.display();

    println!("cargo::rerun-if-changed={path}");
    println!("cargo::rustc-link-search={path}");

    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo::rustc-link-lib=dylib=cef");
        }
        Ok("windows") => {
            // FIXME: Maybe we can get the windows libs from cef cmake?
            const LIBS: &[&str] = &[
                "advapi32", "comdlg32", "dbghelp", "dnsapi", "gdi32", "msimg32", "odbc32",
                "odbccp32", "oleaut32", "shell32", "shlwapi", "user32", "usp10", "uuid", "version",
                "wininet", "winmm", "winspool", "ws2_32", "mincore", "cfgmgr32", "ntdll",
                "onecore", "pdh", "powrprof", "propsys", "setupapi", "shcore", "tbs", "userenv",
                "wbemuuid", "winmm", "delayimp",
            ];
            println!("cargo::rustc-link-lib=libcef_dll_wrapper");
            println!("cargo::rustc-link-lib=dylib=libcef");
            println!("cargo::rustc-link-lib=cef_sandbox");
            for lib in LIBS {
                println!("cargo::rustc-link-lib={lib}");
            }
        }
        Ok("macos") => {
            println!("cargo::rustc-link-lib=framework=AppKit");

            println!("cargo::rustc-link-lib=static=cef_dll_wrapper");
            println!("cargo::rustc-link-lib=cef_sandbox");
            println!("cargo::rustc-link-lib=sandbox");
        }
        os => unimplemented!("unknown target {}", os.unwrap_or("(unset)")),
    }

    Ok(())
}

#[cfg(feature = "dox")]
fn main() {}
