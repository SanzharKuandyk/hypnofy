import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { resolve } from "node:path";

const environment = { ...process.env, CARGO_TARGET_DIR: resolve(".b") };

function run(command, args) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    windowsHide: true,
    env: environment,
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run(process.execPath, ["scripts/build-worker.mjs"]);
run(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", "build", "--debug", "--no-bundle"]);
copyFileSync(resolve(".b/debug/hypnofy.exe"), resolve(".b/debug/Hypnofy-dev.exe"));
console.log(`Development app: ${resolve(".b/debug/Hypnofy-dev.exe")}`);
