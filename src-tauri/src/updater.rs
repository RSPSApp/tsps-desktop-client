use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{Manager, Url, Webview, WebviewWindow};
use tauri::webview::PageLoadEvent;
use tauri_plugin_hot_update::{Config, CurrentBundle, HotUpdateExt, UpdateConfig, UpdateOutcome};

static CHECK_RUNNING: AtomicBool = AtomicBool::new(false);

fn status(window: &WebviewWindow, message: &str, percent: u64) {
    let percent = percent.min(100);
    let message = serde_json::to_string(message).expect("status text serializes");
    if let Err(error) = window.eval(format!("document.getElementById('status').textContent = {message}; document.getElementById('progress').style.width = '{percent}%'; document.querySelector('[role=progressbar]').setAttribute('aria-valuenow', '{percent}');")) {
        eprintln!("Could not display update progress: {error}");
    }
}

fn needs_update(offered: &UpdateOutcome, running: &CurrentBundle) -> bool {
    match offered {
        UpdateOutcome::Available { manifest } => manifest.version > running.version,
        UpdateOutcome::AlreadyStaged { version, .. } => *version > running.version,
        _ => false,
    }
}

type UpdateError = Box<dyn std::error::Error + Send + Sync>;

async fn check(window: &WebviewWindow) -> Result<(), UpdateError> {
    let profile = &window.app_handle().config().plugins.0["desktop-profile"];
    let expected = lookup_build(profile).await?;
    let manifest_url = format!("https://github.com/RSPSApp/tsps/releases/download/client-{expected}/manifest.json");
    let app = window.app_handle();
    let config: Config = serde_json::from_value(app.config().plugins.0["hot-update"].clone())?;
    let source = UpdateConfig {
        manifest_url,
        pubkeys: config.pubkeys,
    };
    let offered = tokio::time::timeout(Duration::from_secs(8), app.hot_update().check(&source)).await??;
    // A trial bundle must reach the game and acknowledge boot before it can be replaced.
    if needs_update(&offered, &app.hot_update().current_bundle()?) {
        status(window, "Downloading client update…", 0);
        let mut last_percent = None;
        let outcome = tokio::time::timeout(Duration::from_secs(300),
            app.hot_update().check_and_download(&source, |downloaded, total| {
                let percent = downloaded.saturating_mul(100) / total.max(1);
                if last_percent != Some(percent) {
                    last_percent = Some(percent);
                    status(window, &format!("Downloading client update… {}%", percent.min(100)), percent);
                }
            })).await??;
        if matches!(outcome, UpdateOutcome::Staged { .. } | UpdateOutcome::AlreadyStaged { .. }) {
            status(window, "Starting updated client…", 100);
            app.hot_update().activate_staged().await?;
        }
    }
    let asset = app.asset_resolver().get("client-release.json".into()).ok_or("Installed client has no build identifier")?;
    let installed: serde_json::Value = serde_json::from_slice(asset.bytes())?;
    if installed["clientVersion"].as_str() != Some(expected.as_str()) {
        return Err("This world requires a different installed client. Automatic downgrade is unavailable.".into());
    }
    window.navigate(window.url()?.join("/")?)?;
    Ok(())
}

fn world_metadata_url(profile: &serde_json::Value) -> Result<Url, UpdateError> {
    if profile["transport"] == "webrtc" {
        let mut url = Url::parse(profile["signalUrl"].as_str().ok_or("Missing relay URL")?)?;
        let scheme = match url.scheme() {
            "ws" => "http",
            "wss" => "https",
            _ => return Err("Invalid relay URL".into()),
        };
        url.set_scheme(scheme).map_err(|_| "Invalid relay URL")?;
        url.set_path("/worlds");
        url.set_query(None);
        url.set_fragment(None);
        let world_id = profile["worldId"].as_str().ok_or("Missing hosted world ID")?;
        url.query_pairs_mut().append_pair("worldId", world_id);
        return Ok(url);
    }

    let scheme = if profile["secure"].as_bool().unwrap_or(false) { "https" } else { "http" };
    let address = profile["address"].as_str().ok_or("Missing world address")?;
    Ok(Url::parse(&format!("{scheme}://{address}"))?.join("/api/world-info")?)
}

async fn lookup_build(profile: &serde_json::Value) -> Result<String, UpdateError> {
    let client = reqwest::Client::builder()
        .tls_backend_native()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut response = client.get(world_metadata_url(profile)?).send().await?;
    if !response.status().is_success() {
        return Err("Could not look up the running world's client build".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > 16 * 1024 {
            return Err("World metadata is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    let data: serde_json::Value = serde_json::from_slice(&body)?;
    let version = data["clientVersion"].as_str().filter(|version| {
        version.len() == 64 && version.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    }).ok_or("This world does not advertise a supported desktop client build")?;
    Ok(version.to_owned())
}

pub fn on_page_load(webview: &Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    if payload.event() != PageLoadEvent::Finished || payload.url().path() != "/desktop-startup.html" { return; }
    let Some(window) = webview.app_handle().get_webview_window("main") else { return; };
    if CHECK_RUNNING.swap(true, Ordering::SeqCst) { return; }
    tauri::async_runtime::spawn(async move {
        let result = check(&window).await;
        CHECK_RUNNING.store(false, Ordering::SeqCst);
        if let Err(error) = result {
            status(&window, &error.to_string(), 0);
            let _ = window.eval("document.getElementById('recovery').hidden = false;");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_hot_update::{BundleSource, Manifest};

    fn available(version: &str) -> UpdateOutcome {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "version": version, "createdAt": "2026-11-19T00:00:00Z", "minShellVersion": "0.1.1",
            "archive": { "url": "https://example.com/client.tar.gz", "sha256": "a".repeat(64), "size": 100 }
        })).unwrap();
        UpdateOutcome::Available { manifest }
    }

    #[test]
    fn metadata_urls_use_http_and_preserve_the_world_id() {
        let direct = serde_json::json!({ "address": "world.example/game?unused=1", "secure": true });
        assert_eq!(world_metadata_url(&direct).unwrap().as_str(), "https://world.example/api/world-info");
        let relayed = serde_json::json!({
            "transport": "webrtc", "signalUrl": "wss://relay.example/signal?unused=1",
            "worldId": "browser-test"
        });
        assert_eq!(world_metadata_url(&relayed).unwrap().as_str(), "https://relay.example/worlds?worldId=browser-test");
    }

    #[test]
    fn trial_bundle_reaches_the_game_without_reactivating() {
        let running = CurrentBundle { source: BundleSource::Ota, seq: Some(1), version: "0.1.2".parse().unwrap() };
        assert!(!needs_update(&available("0.1.2"), &running));
        assert!(needs_update(&available("0.1.3"), &running));
        assert!(!needs_update(&UpdateOutcome::AlreadyStaged { seq: 1, version: "0.1.2".parse().unwrap() }, &running));
    }
}
