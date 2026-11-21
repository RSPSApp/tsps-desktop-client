export function validateConfig(config) {
    if (!config || typeof config !== "object") throw new Error("Expected a client configuration");
    const { name, identifier, serverUrl, worldId, icon } = config;
    if (typeof name !== "string" || !/^[A-Za-z0-9][A-Za-z0-9 ._-]{0,63}$/.test(name)) {
        throw new Error("name must be 1–64 letters, digits, spaces, dots, underscores or hyphens");
    }
    if (typeof identifier !== "string" || !/^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*){2,}$/.test(identifier)) {
        throw new Error("identifier must be a unique reverse-domain name, such as app.rsps.examplescape");
    }
    const url = new URL(serverUrl);
    if (!["ws:", "wss:"].includes(url.protocol) || url.username || url.password || url.hash) {
        throw new Error("serverUrl must be a WebSocket URL without credentials or a fragment");
    }
    if (worldId !== undefined && (typeof worldId !== "string" || !/^[A-Za-z0-9._-]{1,64}$/.test(worldId))) {
        throw new Error("worldId must identify a hosted WebRTC world");
    }
    if (icon !== undefined && (typeof icon !== "string" || !/\.(png|svg)$/i.test(icon) || icon.includes("://"))) {
        throw new Error("icon must be a local PNG or SVG path, relative to the profile JSON");
    }
    return { name, identifier, serverUrl: url.href, ...(worldId === undefined ? {} : { worldId }),
        ...(icon === undefined ? {} : { icon }) };
}

export function getBuildTarget(host, windows = false) {
    if (!["darwin", "win32"].includes(host)) throw new Error("Build on macOS or Windows");
    const isWindows = windows || host === "win32";
    return {
        windows: isWindows,
        bundles: isWindows ? ["nsis"] : ["app", "dmg"],
        args: isWindows && host === "darwin"
            ? ["--runner", "cargo-xwin", "--target", "x86_64-pc-windows-msvc"] : [],
    };
}
