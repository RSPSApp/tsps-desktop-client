import assert from "node:assert/strict";
import test from "node:test";
import { validateConfig } from "./config.mjs";

const config = { name: "ExampleScape", identifier: "app.rsps.examplescape", serverUrl: "wss://play.example.com/game" };
test("accepts a branded client and preserves the game endpoint", () => {
    assert.deepEqual(validateConfig(config), config);
});
test("rejects unsafe branding, identifiers and connection URLs", () => {
    for (const change of [
        { name: "../Other" }, { identifier: "app" }, { serverUrl: "https://play.example.com" },
        { serverUrl: "wss://user:secret@play.example.com" }, { serverUrl: "wss://play.example.com/#other" },
    ]) assert.throws(() => validateConfig({ ...config, ...change }));
});
test("accepts a profile-relative icon and rejects unsupported icon inputs", () => {
    assert.equal(validateConfig({ ...config, icon: "./world-icon.png" }).icon, "./world-icon.png");
    for (const icon of ["", 42, "icon.jpg", "https://example.com/icon.png"]) {
        assert.throws(() => validateConfig({ ...config, icon }), /local PNG or SVG/);
    }
});
test("Windows builds use NSIS and only cross-compile when running on a Mac", async () => {
    const { getBuildTarget } = await import("./config.mjs");
    assert.deepEqual(getBuildTarget("darwin"), { windows: false, bundles: ["app", "dmg"], args: [] });
    assert.deepEqual(getBuildTarget("darwin", true), { windows: true, bundles: ["nsis"], args: ["--runner", "cargo-xwin", "--target", "x86_64-pc-windows-msvc"] });
    assert.deepEqual(getBuildTarget("win32"), { windows: true, bundles: ["nsis"], args: [] });
    assert.throws(() => getBuildTarget("linux"), /macOS or Windows/);
});

test("hosted profiles retain a validated world ID", () => {
    assert.equal(validateConfig({ ...config, worldId: "browser-abc123" }).worldId, "browser-abc123");
    for (const worldId of ["", "../bad", "bad world", 5]) assert.throws(() => validateConfig({ ...config, worldId }), /worldId/);
});
