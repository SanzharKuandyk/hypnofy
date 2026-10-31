import type { TextAnchor, TextRange } from "../../../shared/rpc/generated/bindings";

export interface EditorSelection {
  readonly text: string;

  readonly anchor: TextAnchor | null;

  readonly range: TextRange | null;
}

export async function textHash(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), (value) => value.toString(16).padStart(2, "0")).join(
    "",
  );
}
