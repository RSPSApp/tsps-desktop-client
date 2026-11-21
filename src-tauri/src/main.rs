#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod updater;

fn main() {
    let mut context = tauri::generate_context!();
    let hot_update = tauri_plugin_hot_update::install(&mut context);
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_hot_update::init(hot_update))
        .on_page_load(updater::on_page_load)
        .setup(|app| {
            let profile = app.config().plugins.0.get("desktop-profile")
                .ok_or("Build the desktop client with a server profile")?;
            let script = format!("Object.defineProperty(window, '__RSPS_DESKTOP_PROFILE__', {{ value: Object.freeze({profile}) }});");
            tauri::WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .initialization_script(script)
                .build()?;
            Ok(())
        });
    // shortcut: uses a private WebKit preference; replace when a public high-refresh API is available.
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_macos_fps::init());
    builder.run(context).expect("could not start the desktop client");
}
