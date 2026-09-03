fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_settings",
            "save_settings",
            "get_today_status",
            "mark_checkin_done",
            "open_checkin_window",
        ]),
    ))
    .expect("failed to build Tauri application");
}
