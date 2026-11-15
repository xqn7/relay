fn main() {
    // The problem-report webhook is a secret: it lives next to the updater key on the build
    // PC, never in the repository, and is baked into the build from there. Without the file,
    // reports are simply off.
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let path = std::path::Path::new(&home).join(".relay-keys").join("report-webhook.txt");
        println!("cargo:rerun-if-changed={}", path.display());
        if let Ok(url) = std::fs::read_to_string(&path) {
            println!("cargo:rustc-env=RELAY_REPORT_WEBHOOK={}", url.trim());
        }
    }
    tauri_build::build()
}
