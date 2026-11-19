#!/usr/bin/env node
// MyVideo isolation gate. The original HGhub app remains on main.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

function read(path) { return readFileSync(path, "utf8"); }
const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
const isStandard = tauri.productName === "MyVideo" && tauri.identifier === "com.daneg.myvideo";
const isIsolatedPlaytest = tauri.productName === "MyVideo Playtest" && tauri.identifier === "com.daneg.myvideo.playtest";
const isNativeProbe = tauri.productName === "MyVideo Native Test" && tauri.identifier === "com.daneg.myvideo.nativeprobe";
if (!isStandard && !isIsolatedPlaytest && !isNativeProbe)
  throw new Error("MyVideo identifier does not match approved independent app or playtest");
const required = ["src/App.tsx", "src/main.tsx", "src/pages/SourceHub.tsx",
  "src/sourceHub.ts", "src-tauri/src/lib.rs", "src-tauri/src/net.rs",
  "src-tauri/src/media.rs", "src-tauri/src/source_hub.rs"];
const banned = [/huangguoai\.(?:ai|com)/i, /\/api\/edge\/request/i,
  /api_(?:login|register|feed|search|play)\b/, /from\s+["']\.\.\/api["']/];
for (const p of required) {
  const body = read(p);
  for (const term of banned) {
    if (term.test(body)) throw new Error("Forbidden API reference in " + p + ": " + term);
  }
}
const app = read("src/App.tsx");
if (!app.includes("SourceHub") || app.includes('from "./store"'))
  throw new Error("App is still rendering Huangguo pages");
const items = readdirSync("dist/assets").filter((p) => p.endsWith(".js"));
if (!items.length) throw new Error("No frontend JS built");
for (const name of items) {
  const content = read(join("dist/assets", name));
  for (const term of [/huangguoai\.(?:ai|com)/i, /\/api\/edge\/request/i]) {
    if (term.test(content)) throw new Error("Huangguo API leaked into build: " + name);
  }
}
console.log("MyVideo compiled frontend and native entrypoints verified independent.");
