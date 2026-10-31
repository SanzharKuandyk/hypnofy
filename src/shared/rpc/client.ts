import { Context, Effect, Layer, ManagedRuntime, Queue, Stream, Fiber } from "effect";
import { createTauRPCProxy } from "./generated/bindings";
import { newJobId } from "../../domain/ids";
import type {
  BulkRequest,
  DraftRequest,
  LibraryQuery,
  Preferences,
  ScriptPatch,
  JobSnapshot,
} from "./generated/bindings";

const rpc = createTauRPCProxy();

type WireResult<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };

export class RpcError extends Error {
  readonly code: string;

  constructor(value: unknown) {
    const record = typeof value === "object" && value !== null && "message" in value ? value : null;
    super(
      record && typeof record.message === "string"
        ? record.message
        : value instanceof Error
          ? value.message
          : typeof value === "string"
            ? value
            : "The operation could not be completed",
    );
    this.code =
      record && "code" in record && typeof record.code === "string" ? record.code : "transport";
  }
}

function makeCall<T, E>(operation: () => Promise<WireResult<T, E>>): Effect.Effect<T, RpcError> {
  return Effect.tryPromise({
    try: async () => {
      const result = await operation();
      if (result.status === "error") throw new RpcError(result.error);
      return result.data;
    },
    catch: (value) => (value instanceof RpcError ? value : new RpcError(value)),
  });
}

const Backend = Context.Service<{ readonly call: typeof makeCall }>("HypnofyBackend");

const runtime = ManagedRuntime.make(Layer.succeed(Backend, { call: makeCall }));

function request<T, E>(operation: () => Promise<WireResult<T, E>>): Promise<T> {
  return runtime.runPromise(Effect.flatMap(Backend, (service) => service.call(operation)));
}

export const api = {
  entry: {
    status: () => rpc.entry.status(),
    accept: (version: string, acknowledged: boolean) =>
      request(() => rpc.entry.accept(version, acknowledged)),
    exit: () => rpc.entry.exit(),
  },
  library: {
    collections: () => request(() => rpc.library.collections()),
    createCollection: (name: string) => request(() => rpc.library.create_collection(name)),
    renameCollection: (id: string, name: string) =>
      request(() => rpc.library.rename_collection(id, name)),
    deleteCollection: (id: string) => request(() => rpc.library.delete_collection(id)),
    scripts: (query: LibraryQuery) => request(() => rpc.library.scripts(query)),
  },
  scripts: {
    exportMany: (ids: string[], directory: string, format: "txt" | "md" | "json") =>
      request(() => rpc.scripts.export_many(ids, directory, format, newJobId())),
    get: (id: string) => request(() => rpc.scripts.get(id)),
    create: (collection: string | null) => request(() => rpc.scripts.create(collection)),
    save: (patch: ScriptPatch) => request(() => rpc.scripts.save(patch)),
    duplicate: (id: string) => request(() => rpc.scripts.duplicate(id)),
    bulk: (requestValue: BulkRequest) => request(() => rpc.scripts.bulk(requestValue)),
    revisions: (id: string) => request(() => rpc.scripts.revisions(id)),
    restoreRevision: (id: string, revision: number) =>
      request(() => rpc.scripts.restore_revision(id, revision)),
    importFile: (path: string, collection: string | null) =>
      request(() => rpc.scripts.import_file(path, collection)),
    exportFile: (id: string, path: string) => request(() => rpc.scripts.export_file(id, path)),
    legacyDocument: (id: string) => request(() => rpc.scripts.legacy_document(id)),
  },
  settings: {
    get: () => request(() => rpc.settings.get()),
    save: (preferences: Preferences) => request(() => rpc.settings.save(preferences)),
    storage: () => request(() => rpc.settings.storage()),
    cleanup: () => request(() => rpc.settings.cleanup()),
  },
  models: {
    usage: (id: string) => request(() => rpc.models.usage(id)),
    list: () => request(() => rpc.models.list()),
    catalog: () => rpc.models.catalog(),
    scan: () => request(() => rpc.models.scan(newJobId())),
    link: (path: string, architecture: string | null, id: string | null = null) =>
      request(() => rpc.models.link(path, architecture, id)),
    verify: (id: string, jobId: string) => request(() => rpc.models.verify(id, jobId)),
    plan: (id: string) => request(() => rpc.models.plan(id)),
    install: (planId: string, jobId: string) => request(() => rpc.models.install(planId, jobId)),
    remove: (id: string) => request(() => rpc.models.remove(id)),
  },
  jobs: {
    list: () => request(() => rpc.jobs.list()),
    snapshot: (id: string) => request(() => rpc.jobs.snapshot(id)),
    cancel: (id: string) => request(() => rpc.jobs.cancel(id)),
    onChanged: rpc.jobs.changed.on,
  },
  generation: {
    start: (value: DraftRequest) => request(() => rpc.generation.start(value)),
    suggestions: (id: string) => request(() => rpc.generation.suggestions(id)),
    discard: (id: string) => request(() => rpc.generation.discard(id)),
  },
  narration: {
    plan: (id: string) => request(() => rpc.narration.plan(id)),
    render: (id: string, jobId: string, preview = false, blockIds: string[] = []) =>
      request(() => rpc.narration.render(id, jobId, preview, blockIds)),
    renders: (id: string) => request(() => rpc.narration.renders(id)),
    export: (id: string, path: string, ambience: string | null) =>
      request(() => rpc.narration.export(id, path, ambience, newJobId())),
  },
  playback: {
    state: () => request(() => rpc.playback.state()),
    load: (id: string) => request(() => rpc.playback.load(id)),
    control: (operation: string, value: number | null = null) =>
      request(() => rpc.playback.control(operation, value)),
    ambience: () => request(() => rpc.playback.ambience()),
    importAmbience: (path: string) => request(() => rpc.playback.import_ambience(path)),
    selectAmbience: (id: string | null) => request(() => rpc.playback.select_ambience(id)),
  },
  providers: {
    connected: () => rpc.providers.connected(),
    connect: (key: string) => request(() => rpc.providers.connect(key)),
    disconnect: () => request(() => rpc.providers.disconnect()),
    refresh: () => request(() => rpc.providers.refresh()),
  },
};

export function disposeRuntime(): Promise<void> {
  return runtime.dispose();
}

export async function subscribeJobs(
  onSnapshot: (snapshot: JobSnapshot) => void,
): Promise<() => void> {
  let ready: () => void = () => {};
  let reject: (value: unknown) => void = () => {};
  const subscribed = new Promise<void>((resolve, failure) => {
    ready = resolve;
    reject = failure;
  });
  const events = Stream.callback<JobSnapshot, RpcError>(
    (queue) =>
      Effect.acquireRelease(
        Effect.tryPromise({
          try: async () => {
            try {
              const unlisten = await rpc.jobs.changed.on((snapshot) => {
                Queue.offerUnsafe(queue, snapshot);
              });
              ready();
              return unlisten;
            } catch (value) {
              reject(value);
              throw value;
            }
          },
          catch: (value) => new RpcError(value),
        }),
        (unlisten) => Effect.sync(unlisten),
      ),
    { bufferSize: 128, strategy: "sliding" },
  );
  const fiber = runtime.runFork(
    Stream.runForEach(events, (snapshot) => Effect.sync(() => onSnapshot(snapshot))),
  );
  await subscribed;
  return () => {
    void runtime.runPromise(Fiber.interrupt(fiber));
  };
}
