import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { validateConfig, getBuildTarget } from "./config.mjs";

const desktop = dirname(fileURLToPath(import.meta.url));
const { values, positionals } = parseArgs({ allowPositionals: true, options: { windows: { type: "boolean", default: false } } });
if (positionals.length !== 1) throw new Error("Usage: npm run build -- <profile.json> [--windows]");
const target = getBuildTarget(process.platform, values.windows);
const profilePath = resolve(positionals[0]);
const config = validateConfig(JSON.parse(readFileSync(profilePath, "utf8")));
const url = new URL(config.serverUrl);
const server = { name: config.name, address: `${url.host}${url.pathname}${url.search}`, secure: url.protocol === "wss:", maxPlayers: 2047,
    ...(config.worldId ? { transport: "webrtc", signalUrl: config.serverUrl, worldId: config.worldId, iceServers: [{ urls: "stun:stun.rsps.app:3478" }] } : {}) };
const env = { ...process.env };
function run(command, args, cwd) {
    const result = spawnSync(command, args, { cwd, env, stdio: "inherit" });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status ?? 1);
}
const tauri = resolve(desktop, "node_modules/@tauri-apps/cli/tauri.js");
let icons;
if (config.icon || target.windows) {
    const iconPath = config.icon ? resolve(dirname(profilePath), config.icon) : resolve(desktop, "src-tauri/icons/icon.png");
    if (!existsSync(iconPath) || !statSync(iconPath).isFile()) throw new Error(`Icon not found: ${iconPath}`);
    run(process.execPath, [tauri, "icon", iconPath, "--output", resolve(desktop, "src-tauri/branding-icons")], desktop);
    icons = [target.windows ? "branding-icons/icon.ico" : "branding-icons/icon.icns", "branding-icons/icon.png"];
}
const override = resolve(desktop, "src-tauri/branding.local.json");
writeFileSync(override, JSON.stringify({ productName: config.name, identifier: config.identifier,
    bundle: { targets: target.bundles, ...(icons ? { icon: icons } : {}) },
    plugins: { "desktop-profile": server },
    app: { windows: [{ title: config.name, create: false, url: "desktop-startup.html", width: 1280, height: 800, minWidth: 765, minHeight: 503 }] } }));
run(process.execPath, [tauri, "build", "--config", override, ...target.args], desktop);
