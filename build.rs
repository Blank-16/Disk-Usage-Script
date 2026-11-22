fn main() {
    println!("cargo:rerun-if-changed=assets/cat.ico");
    println!("cargo:rerun-if-changed=assets/cat.rc");
    println!("cargo:rerun-if-changed=assets/cat.res");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let res_path = format!("{}/assets/cat.res", manifest_dir);

        let rc_compiled = if let Ok(status) = std::process::Command::new("rc.exe")
            .args(["/fo", &res_path, "assets/cat.rc"])
            .current_dir(&manifest_dir)
            .status()
        {
            status.success()
        } else {
            false
        };

        let _ = rc_compiled;

        if std::path::Path::new(&res_path).exists() {
            println!("cargo:rustc-link-arg={}", res_path);
        }
    }
}
