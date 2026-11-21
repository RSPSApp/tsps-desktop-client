Based on crates.io tauri-plugin-hot-update 0.1.1 (MIT OR Apache-2.0).

Local patch: native-only activate_staged() switches the shared asset provider before
startup navigates into the game. It persists the existing trial/rollback state,
rejects replacing an unacknowledged trial, and retains the same update mutex across
activations. No activation IPC command is exposed to downloaded JavaScript.

Only call this from the embedded startup page before game assets are requested.
Remove this vendor patch when upstream provides equivalent in-process activation.
