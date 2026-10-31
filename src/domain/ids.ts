export type Brand<T, Name extends string> = T & { readonly __brand: Name };

export type ScriptId = Brand<string, "ScriptId">;

export type ModelId = Brand<string, "ModelId">;

export type JobId = Brand<string, "JobId">;

export type CollectionId = Brand<string, "CollectionId">;

export type BlockId = Brand<string, "BlockId">;

export type RenderId = Brand<string, "RenderId">;

export type Revision = Brand<number, "Revision">;

export type Milliseconds = Brand<number, "Milliseconds">;

export type ContentHash = Brand<string, "ContentHash">;

export function identifier<Name extends string>(value: string, _name: Name): Brand<string, Name> {
  if (!value || value.length > 256 || value.includes("\0"))
    throw new Error(`Invalid ${_name} identifier`);
  // The constructor validates the wire value before introducing its nominal type.
  return value as Brand<string, Name>;
}

export function newJobId(): JobId {
  return identifier(crypto.randomUUID(), "JobId");
}

export function milliseconds(value: number): Milliseconds {
  if (!Number.isSafeInteger(value) || value < 0 || value > 4_294_967_295)
    throw new Error("Invalid millisecond value");
  return value as Milliseconds;
}

export function revision(value: number): Revision {
  if (!Number.isSafeInteger(value) || value < 1) throw new Error("Invalid document revision");
  return value as Revision;
}

export function contentHash(value: string): ContentHash {
  if (!/^[a-f0-9]{64}$/.test(value)) throw new Error("Invalid content hash");
  return value as ContentHash;
}
