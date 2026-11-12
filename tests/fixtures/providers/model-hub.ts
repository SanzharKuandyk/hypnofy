import { createServer } from "node:http";
import type { Server } from "node:http";

export interface ModelHubFixture {
  readonly url: string;

  readonly requests: string[];

  readonly close: () => Promise<void>;
}

export async function startModelHubFixture(): Promise<ModelHubFixture> {
  const requests: string[] = [];
  const sha = "a".repeat(40);
  const qwen = {
    id: "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
    pipeline_tag: "text-to-speech",
    library_name: "transformers",
    downloads: 1500000,
    likes: 400,
    sha,
    tags: ["text-to-speech", "license:apache-2.0"],
    cardData: { license: "apache-2.0", language: ["en", "zh"] },
    siblings: [
      { rfilename: "README.md", size: 350 },
      { rfilename: "model.safetensors", size: 3456789012 },
    ],
  };
  const server: Server = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    requests.push(url.pathname + url.search);
    response.setHeader("Content-Type", "application/json");
    if (url.pathname === "/api/models") {
      if (url.searchParams.has("cursor")) {
        response.end(JSON.stringify([{ ...qwen, id: "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign" }]));
      } else {
        const address = server.address();
        if (!address || typeof address === "string") throw Error("No fixture address");
        response.setHeader(
          "Link",
          `<http://127.0.0.1:${address.port}/api/models?cursor=next>; rel="next"`,
        );
        response.end(
          JSON.stringify([
            qwen,
            {
              id: "Supertone/supertonic-3",
              pipeline_tag: "text-to-speech",
              downloads: 25000,
              cardData: { license: "openrail" },
            },
          ]),
        );
      }
    } else if (url.pathname.startsWith("/api/models/Qwen/")) {
      response.end(JSON.stringify(qwen));
    } else if (url.pathname.endsWith("/README.md")) {
      response.setHeader("Content-Type", "text/plain");
      response.end(
        '---\nlicense: apache-2.0\n---\n# Qwen3-TTS\n\nSynthetic voice model card for browsing tests.\n\n## Voice instructions\n\nDescribe the pace and expression in a prompt.\n\n```python\nmodel.generate(text="Breathe gently")\n```\n\n<script>window.cardExecuted=true</script>\n\n' +
          "More model documentation.\n\n".repeat(40),
      );
    } else {
      response.statusCode = 404;
      response.end("{}");
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw Error("No fixture address");
  return {
    url: `http://127.0.0.1:${address.port}/`,
    requests,
    close: () =>
      new Promise<void>((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      ),
  };
}
