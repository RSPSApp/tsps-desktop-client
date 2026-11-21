# TSPS Desktop Client

Tauri launcher for TSPS worlds that downloads and verifies the signed client build each world requires. The installer ships the launcher. It fetches the game client on first launch.

## Build

Install Node.js 22 or newer, Rust, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. Copy `config.example.json` and set your world’s name, unique app identifier, and WebSocket URL. For a browser-hosted world, use the relay URL in `serverUrl` and add its `worldId`. Set `icon` to a profile-relative PNG or SVG to use your own icon.

```sh
npm ci
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run build -- config.example.json
```

Builds produce a macOS app and DMG or a Windows NSIS installer under `src-tauri/target`. The example profile uses a placeholder world address.

## How updates work

Direct worlds advertise their build ID at `/api/world-info`. For a relayed world, the launcher requests `/worlds?worldId=<id>`. The launcher uses that ID to select a release from [RSPSApp/tsps](https://github.com/RSPSApp/tsps/releases), then verifies the signed manifest and installed bundle before opening the game. The updater currently accepts forward releases.

The world and its relay must support build ID discovery. See [TSPS #6408](https://github.com/RSPSApp/tsps/pull/6408) and [relay #43](https://github.com/tobywisener/rsps-website/pull/43).
