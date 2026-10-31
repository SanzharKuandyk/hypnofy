import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const result = spawnSync(
  "cargo",
  [
    "run",
    "--quiet",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "-p",
    "hypnofy",
    "--bin",
    "export-rpc",
    "--",
    ...process.argv.slice(2),
  ],
  { stdio: "inherit", windowsHide: true, env: { ...process.env, CARGO_TARGET_DIR: resolve(".b") } },
);
process.exitCode = result.status ?? 1;
