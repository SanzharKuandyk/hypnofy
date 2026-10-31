import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

const args = process.argv.slice(2);
if (args[0] === "build" && !args.includes("--debug")) {
  throw new Error("Release packaging is disabled during the rewrite. Use npm run dev:exe.");
}
if (args[0] === "dev") {
  const worker = spawnSync(process.execPath, ["scripts/build-worker.mjs"], {
    stdio: "inherit",
    windowsHide: true,
  });
  if (worker.status !== 0) process.exit(worker.status ?? 1);
}
const child = spawn(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", ...args], {
  stdio: "inherit",
  windowsHide: true,
  env: { ...process.env, CARGO_TARGET_DIR: resolve(".b") },
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
