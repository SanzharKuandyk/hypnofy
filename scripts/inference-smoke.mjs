import { spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { once } from "node:events";
import assert from "node:assert/strict";

// Model locations are explicit arguments; never scan application or user data.
const args = process.argv.slice(2);
const option = (name) => {
  const index = args.indexOf(name);
  return index < 0 ? undefined : args[index + 1];
};

const device = option("--device") ?? "cpu";
assert(device === "cpu" || device === "auto");

const binary = resolve(".b/debug/hypnofy-inference-worker.exe");
await access(binary);
const root = await mkdtemp(join(tmpdir(), "hypnofy-worker-smoke-"));
const child = spawn(binary, [root], {
  env: { ...process.env, NOBODYWHO_ESPEAK_DATA_DIR: join(root, "espeak") },
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});

const closed = once(child, "close");
// Attach immediately: spawn can fail before the first request awaits close.
void closed.catch(() => undefined);
let buffer = Buffer.alloc(0);
let pending;
let sequence = 0;
let activeId = "";
let firstDelta;

function rejectPending(error) {
  pending?.reject(error);
  pending = undefined;
}

child.on("error", rejectPending);
child.stdin.on("error", rejectPending);
child.on("close", () => rejectPending(new Error("Worker exited before completing the request")));
child.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  try {
    while (buffer.length >= 4) {
      const length = buffer.readUInt32LE();
      assert(length > 0 && length <= 4 * 1024 * 1024);
      if (buffer.length < length + 4) return;
      const event = JSON.parse(buffer.subarray(4, length + 4).toString("utf8"));
      buffer = buffer.subarray(length + 4);
      assert.equal(event.version, 2);
      assert.equal(event.jobId, activeId);
      assert.equal(event.sequence, ++sequence);
      if (event.payload.kind === "failed") throw new Error(event.payload.message);
      if (event.payload.kind === "model-info") {
        assert(event.payload.context_tokens >= 512);
      } else if (event.payload.kind === "text-delta") {
        pending?.deltas.push(event.payload.text);
        firstDelta?.();
        firstDelta = undefined;
      } else {
        pending?.resolve({ ...event.payload, deltas: pending.deltas.join("") });
        pending = undefined;
      }
    }
  } catch (error) {
    rejectPending(error);
    child.kill();
  }
});

function request(jobId, operation) {
  activeId = jobId;
  sequence = 0;
  return new Promise((resolveResult, reject) => {
    pending = { resolve: resolveResult, reject, deltas: [] };
    const payload = Buffer.from(JSON.stringify({ version: 2, jobId, operation }));
    const length = Buffer.alloc(4);
    length.writeUInt32LE(payload.length);
    child.stdin.write(Buffer.concat([length, payload]));
  });
}

const timeout = setTimeout(() => {
  rejectPending(new Error("Native smoke test timed out"));
  child.kill();
}, 180_000);

try {
  const ping = await request("smoke-ping", { kind: "ping" });
  assert.equal(ping.kind, "ready");
  console.log("PASS: framed protocol handshake");

  const textModel = option("--text-model");
  if (textModel) {
    const result = await request("smoke-text", {
      kind: "generate",
      model_path: resolve(textModel),
      prompt: "Write one short sentence inviting the listener to relax.",
      system_prompt: "Write only one spoken sentence. Do not use tools.",
      context_tokens: 2048,
      device,
    });
    assert.equal(result.kind, "text-completed");
    assert(result.text.trim().length > 0);
    assert(!result.text.includes("<think>"), "Writing output must not contain model reasoning blocks");
    assert(result.deltas.length > 0);
    console.log(`PASS: local ${device} text generation and streaming`);
  }

  const speechModel = option("--speech-model");
  if (speechModel) {
    const result = await request("smoke-speech", {
      kind: "synthesize",
      model_path: resolve(speechModel),
      architecture: "kokoro",
      voice: "bf_emma",
      language: "en-gb",
      speed: 1,
      text: "Let your shoulders settle and take a gentle breath.",
      device,
    });
    assert.equal(result.kind, "audio-completed");
    assert.equal(result.filename, "smoke-speech.wav");
    const wav = await readFile(join(root, result.filename));
    assert.equal(wav.subarray(0, 4).toString(), "RIFF");
    assert.equal(wav.subarray(8, 12).toString(), "WAVE");
    assert(wav.length > 44);
    console.log(`PASS: local Kokoro ${device} synthesis`);
  }

  if (!textModel || !speechModel) {
    console.log(
      "Real-model gate incomplete: pass both --text-model and --speech-model with explicit fixture locations.",
    );
  }
  if (args.includes("--test-cancellation") && textModel) {
    const started = new Promise((resolveStarted) => {
      firstDelta = resolveStarted;
    });
    const completion = request("smoke-cancel", {
      kind: "generate",
      model_path: resolve(textModel),
      prompt: "Write a long relaxing story with twenty paragraphs.",
      system_prompt: "Write only the story. Do not use tools.",
      context_tokens: 2048,
      device,
    });
    void completion.catch(() => undefined);
    await Promise.race([
      started,
      completion.then(() => {
        throw new Error("Generation ended before cancellation");
      }),
    ]);
    child.kill();
    await closed;
    console.log("PASS: parent can terminate active native generation");
  } else {
    child.stdin.end();
    const [code] = await closed;
    assert.equal(code, 0);
  }
} finally {
  clearTimeout(timeout);
  if (child.exitCode === null) child.kill();
  await closed.catch(() => undefined);
  await rm(root, { recursive: true, force: true });
}
