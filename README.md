# TSPS Desktop Client

Tauri launcher for TSPS worlds. The installer contains the launcher, which downloads and verifies the signed game client on first launch.

## Create your world profile

Save a JSON file such as `world.json` in this checkout. Start with [config.example.json](config.example.json) and replace the example values.

For a direct world:

```json
{
  "name": "ExampleScape",
  "identifier": "app.rsps.examplescape",
  "serverUrl": "wss://play.example.com/game"
}
```

For a browser-hosted world on the relay:

```json
{
  "name": "ExampleScape Hosted",
  "identifier": "app.rsps.examplescapehosted",
  "serverUrl": "wss://worlds.rsps.app/signal",
  "worldId": "browser-2e3e5128-4fa4-40fd-be49-9dd6000bae26"
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | Yes | App and window name. Use 1–64 characters: letters, digits, spaces, dots, underscores, or hyphens. Start with a letter or digit. |
| `identifier` | Yes | Unique reverse-domain app ID, such as `app.rsps.examplescape`. Use at least three dot-separated parts of lowercase letters and digits, each starting with a letter. Keep it stable across builds so the launcher can reuse its saved data. |
| `serverUrl` | Yes | The world's `ws://` or `wss://` game endpoint. For a relayed world, set this to its WebSocket relay instead. URLs cannot include credentials or a fragment. |
| `worldId` | Relayed worlds | The actual world ID from its `/host` play link or the relay's `/worlds` directory. Use 1–64 letters, digits, dots, underscores, or hyphens. Omit this field for direct worlds. |
| `icon` | No | Path to a PNG or SVG relative to the profile file, such as `./world-icon.png`. A custom icon is optional. |

## Build installers

From the repository root, install Node.js 22 or newer, Rust, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for the build machine, then run `npm ci`. Use your own profile in the commands below.

### macOS app and DMG

Run on macOS with the Xcode Command Line Tools installed:

```sh
npm run build -- world.json
```

The `.app` is under `src-tauri/target/release/bundle/macos/` and the `.dmg` is under `src-tauri/target/release/bundle/dmg/`. The build uses the machine's architecture.

### Windows installer on Windows

Run in PowerShell with the Microsoft C++ Build Tools and WebView2 prerequisites installed:

```powershell
npm run build -- world.json
```

The NSIS `-setup.exe` is under `src-tauri/target/release/bundle/nsis/`.

### 64-bit Windows installer from a Mac

Install Homebrew LLVM, the Windows Rust target, and `cargo-xwin` once. See [Tauri's Windows cross-compilation guide](https://v2.tauri.app/distribute/windows-installer/#cross-compilation).

```sh
brew install llvm
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
export PATH="$(brew --prefix llvm)/bin:$PATH"
npm run build -- world.json --windows
```

The NSIS `-setup.exe` is under `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`.

### Run the tests

```sh
npm test
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## How updates work

Direct worlds expose their build ID at `/api/world-info`. For a relayed world, the launcher requests `/worlds?worldId=<id>`. It uses that ID to select a release from [RSPSApp/tsps](https://github.com/RSPSApp/tsps/releases), then verifies the signed manifest and installed bundle before opening the game. The updater currently accepts forward releases.

Before testing first launch, check that the running world advertises its build ID and that the matching signed release exists. Relayed worlds also need the updated host and relay. See [TSPS #6408](https://github.com/RSPSApp/tsps/pull/6408) and [website #43](https://github.com/tobywisener/rsps-website/pull/43).
