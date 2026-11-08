fn main() {
    // Every command the UI may call must be listed here AND granted in
    // capabilities/main.json. Anything else is rejected by Tauri.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "list_projects",
        "inspect_project",
        "list_fonts",
        "get_settings",
        "save_settings",
        "setup_state",
        "complete_setup",
        "key_status",
        "save_api_key",
        "delete_api_key",
        "get_glossary",
        "save_glossary",
        "restore_backup",
        "get_usage",
        "clear_usage",
        "set_price",
        "app_info",
        "open_link",
        "check_update",
        "install_update",
        "get_update_settings",
        "set_auto_update_check",
        "start_caption",
    ]);
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)) {
        eprintln!("tauri-build failed: {e:#}");
        std::process::exit(1);
    }
}
