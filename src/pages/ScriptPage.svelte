<script lang="ts">
  import { onMount } from "svelte";
  import {
    ChevronLeft,
    Star,
    MoreHorizontal,
    Mic,
    PenLine,
    Sparkles,
    SlidersHorizontal,
    Download,
    Copy,
    Trash2,
    History,
    Link2,
  } from "@lucide/svelte";
  import { Button, Badge, Dialog, Field } from "../shared/ui";
  import { loadScriptEditor } from "../modules/editor";
  import type { EditorSelection } from "../modules/editor";
  import { GenerationPanel } from "../modules/generation";
  import { StickyPlayer } from "../modules/playback";
  import { api } from "../shared/rpc/client";
  import { chooseExport, chooseAmbience } from "../shared/platform/dialogs";
  import { countWords, textOf } from "../domain/document";
  import { newJobId } from "../domain/ids";
  import type { DeliveryOverride } from "../domain/document";
  import type { Workspace } from "../app/workflows/workspace.svelte";
  import type { Suggestion, RevisionRecord } from "../domain";

  interface EditorActions {
    applySuggestion(text: string, replace: boolean, suggestion: Suggestion): void;
    applyDelivery(value: DeliveryOverride): void;
    makeContinuous(): void;
  }

  let { workspace: w }: { workspace: Workspace } = $props();
  const editorComponent = loadScriptEditor();
  let editorView = $state<EditorActions>();
  let tab = $state("write");
  let panel = $state("outline");
  let sheet = $state(false);
  let menu = $state(false);
  let revisions = $state<RevisionRecord[]>([]);
  let revisionOpen = $state(false);
  let selectedText = $state("");
  let selection = $state<EditorSelection>({ text: "", anchor: null, range: null });
  let hasSelection = $state(false);
  let writingModel = $state("");
  let overrideVoice = $state("");
  let overrideSpeed = $state(0.85);
  let generationSequence = $state(0);
  let generationJobId = $state("");
  let renderJobId = $state("");
  let exportOpen = $state(false);
  let exportMixed = $state(false);
  let ambienceId = $state("");
  let chosenRender = $state("");
  const script = $derived(w.editor.script);
  const voices = $derived(
    w.models
      .filter((m) => m.capability === "speech" && m.status === "ready")
      .flatMap((m) => m.voices),
  );
  const render = $derived(
    w.renders.find((r) => r.id === chosenRender) ??
      w.renders.find((r) => r.id === w.playback?.renderId) ??
      w.renders.find((r) => !r.preview),
  );
  const job = $derived(w.jobs.find((j) => j.id === generationJobId) ?? null);
  const renderJob = $derived(w.jobs.find((j) => j.id === renderJobId));
  let playedPreview = "";
  const previewId = $derived(
    renderJob?.result?.kind === "render" ? renderJob.result.renderId : null,
  );
  $effect(() => {
    const id = previewId;
    if (
      renderJob?.state === "completed" &&
      id &&
      id !== playedPreview &&
      w.renders.some((r) => r.id === id && r.preview)
    ) {
      playedPreview = id;
      chosenRender = id;
      void w.action(async () => {
        await api.playback.load(id);
        await api.playback.control("play");
      });
    }
  });
  const rendering = $derived(
    renderJob && ["queued", "running", "cancelling"].includes(renderJob.state),
  );
  const foreignPlayback = $derived(
    !!w.playback?.renderId && !w.renders.some((render) => render.id === w.playback?.renderId),
  );
  const stale = $derived(w.editor.editSequence !== generationSequence);
  const paragraphs = $derived(w.editor.document.content);

  onMount(() => {
    writingModel =
      [w.editor.script?.lastModelId, w.preferences?.textModelId].find((id) =>
        w.models.some(
          (model) => model.id === id && model.status === "ready" && model.capability === "text",
        ),
      ) ?? "";
    ambienceId = w.editor.script?.session.ambienceId ?? "";
  });

  function ask(snapshot: EditorSelection): void {
    selection = snapshot;
    selectedText = snapshot.text;
    panel = "ai";
    generationSequence = w.editor.editSequence;
    sheet = window.innerWidth < 1024;
  }

  async function generate(instruction: string, operation: string): Promise<void> {
    await w.action(async () => {
      await w.editor.flush();
      if (!w.editor.script) return;
      generationSequence = w.editor.editSequence;
      generationJobId = newJobId();
      await api.generation.start({
        jobId: generationJobId,
        scriptId: w.editor.script.id,
        baseRevision: w.editor.script.revision,
        baseContentHash: w.editor.script.contentHash,
        modelId: writingModel,
        operation,
        instruction,
        selectedText,
        anchor: selection.anchor,
        range: selection.range,
      });
      await w.poll();
    });
  }

  function apply(suggestion: Suggestion, replace: boolean): void {
    void w.action(async () => {
      if (
        w.editor.editSequence !== generationSequence ||
        suggestion.baseRevision !== w.editor.script?.revision ||
        suggestion.baseContentHash !== w.editor.script.contentHash
      )
        throw new Error(
          "The script changed. Generate a fresh suggestion or copy this text for manual review.",
        );
      editorView?.applySuggestion(suggestion.text, replace, suggestion);
      if (w.editor.script) w.editor.script.lastModelId = suggestion.modelId;
      await w.editor.flush();
      await api.generation.discard(suggestion.id);
      w.suggestions = w.suggestions.filter((s) => s.id !== suggestion.id);
      sheet = false;
    });
  }

  async function startRender(preview = false, blockIds: string[] = []): Promise<void> {
    await w.editor.flush();
    if (!script) return;
    renderJobId = newJobId();
    await api.narration.render(script.id, renderJobId, preview, blockIds);
    await w.poll();
  }

  async function play(): Promise<void> {
    if (!render) return;
    if (w.playback?.renderId !== render.id) {
      await api.playback.load(render.id);
      if (script) {
        await api.playback.control("volume", script.session.narrationVolume ?? 1);
        await api.playback.control("ambience-volume", script.session.ambienceVolume ?? 0.06);
        await api.playback.selectAmbience(script.session.ambienceId);
      }
    }
    await api.playback.control(
      w.playback?.playing && w.playback.renderId === render.id ? "pause" : "play",
    );
    await w.poll();
  }

  async function recoveryCopy(): Promise<void> {
    if (!script) return;
    const copy = await api.scripts.create(script.collectionId);
    const saved = await api.scripts.save({
      id: copy.id,
      title: `${script.title} (recovered copy)`,
      document: $state.snapshot(w.editor.document),
      lastModelId: script.lastModelId,
      collectionId: script.collectionId,
      favorite: script.favorite,
      expectedRevision: copy.revision,
      session: $state.snapshot(script.session),
    });
    w.editor.load(saved);
    w.route = { kind: "script", id: saved.id };
    await w.refreshLibrary();
  }
</script>

{#snippet delivery()}
  {#if script}
    <div class="row spread">
      <h2>Delivery</h2>
      <Badge>Local speech</Badge>
    </div>
    <p class="caption muted">Start with script defaults, then shape individual passages.</p>
    <Field label="Script voice"
      ><select
        aria-label="Script voice"
        value={script.session.voiceId}
        onchange={(event) => w.editor.session({ voiceId: event.currentTarget.value || null })}
        ><option value="">Choose a voice</option>{#each voices as voice (voice.id)}<option
            value={voice.id}>{voice.name} · {voice.language}</option
          >{/each}</select
      ></Field
    >
    {#if !voices.length}<p class="caption muted">Install a speech model to enable voices.</p>
      <Button onclick={() => w.action(() => w.navigate({ kind: "models" }))}>Get a voice</Button
      >{/if}
    <Field label={`Script pace · ${(script.session.speed ?? 1).toFixed(2)}×`}
      ><input
        aria-label="Script pace"
        type="range"
        min=".5"
        max="2"
        step=".05"
        value={script.session.speed}
        oninput={(event) => w.editor.session({ speed: Number(event.currentTarget.value) })}
      /></Field
    >
    <Field label="Paragraph pause · seconds"
      ><input
        aria-label="Paragraph pause"
        type="number"
        min="0"
        max="30"
        step=".5"
        value={script.session.paragraphPauseMs / 1000}
        onchange={(event) =>
          w.editor.session({
            paragraphPauseMs: Math.round(Number(event.currentTarget.value) * 1000),
          })}
      /></Field
    >
    <hr style="border:0;border-top:1px solid var(--line);width:100%" />
    <h3>{hasSelection ? "Selected text" : "Current passage"}</h3>
    <Field label="Voice override"
      ><select aria-label="Passage voice" bind:value={overrideVoice}
        ><option value="">Inherit script voice</option>{#each voices as voice (voice.id)}<option
            value={voice.id}>{voice.name}</option
          >{/each}</select
      ></Field
    >
    <Field label={`Pace override · ${overrideSpeed.toFixed(2)}×`}
      ><input
        aria-label="Passage pace"
        type="range"
        min=".5"
        max="2"
        step=".05"
        bind:value={overrideSpeed}
      /></Field
    >
    <Button
      onclick={() => {
        editorView?.applyDelivery({ voiceId: overrideVoice || null, speed: overrideSpeed });
        sheet = false;
      }}>Apply to {hasSelection ? "selection" : "passage"}</Button
    >
    <Button
      variant="ghost"
      onclick={() => editorView?.applyDelivery({ voiceId: null, speed: null })}
      >Reset to script defaults</Button
    >
    <Button variant="ghost" onclick={() => editorView?.makeContinuous()}
      ><Link2 size={15} />Make selection continuous</Button
    >
    <p class="caption muted">
      Continuous delivery removes extra paragraph gaps. Natural speech pauses remain.
    </p>
    <hr style="border:0;border-top:1px solid var(--line);width:100%" />
    <Field label="Ambience"
      ><select
        aria-label="Ambience track"
        bind:value={ambienceId}
        onchange={() =>
          w.action(async () => {
            w.editor.session({ ambienceId: ambienceId || null });
            await api.playback.selectAmbience(ambienceId || null);
          })}
        ><option value="">None</option>{#each w.ambience as ambient (ambient.id)}<option
            value={ambient.id}>{ambient.name}</option
          >{/each}</select
      ></Field
    >
    <Button
      variant="ghost"
      onclick={() =>
        w.action(async () => {
          const path = await chooseAmbience();
          if (path) {
            await api.playback.importAmbience(path);
            await w.refresh();
          }
        })}>Import ambience</Button
    >
    <Field label="Ambience volume"
      ><input
        aria-label="Ambience volume"
        type="range"
        min="0"
        max="1"
        step=".01"
        value={w.playback?.ambienceVolume ?? script.session.ambienceVolume}
        oninput={(event) => {
          const volume = Number(event.currentTarget.value);
          w.editor.session({ ambienceVolume: volume });
          void w.action(async () => {
            await api.playback.control("ambience-volume", volume);
          });
        }}
      /></Field
    >
  {/if}
{/snippet}

{#snippet ai()}
  <GenerationPanel
    models={w.models}
    suggestions={w.suggestions}
    {job}
    {selectedText}
    bind:modelId={writingModel}
    baseRevision={script?.revision ?? 0}
    baseContentHash={script?.contentHash ?? ""}
    onstart={generate}
    onapply={apply}
    ondiscard={(id) =>
      w.action(async () => {
        await api.generation.discard(id);
        w.suggestions = w.suggestions.filter((s) => s.id !== id);
      })}
    onclose={() => {
      panel = "delivery";
      sheet = false;
    }}
    {stale}
  />
{/snippet}

{#snippet outline()}
  <span class="caption muted">ON THIS PAGE</span>
  {#each w.editor.document.content.filter((block) => block.type === "heading") as heading (heading.attrs?.id)}<Button
      variant="ghost"
      onclick={() =>
        document
          .querySelector(`[data-block-id="${CSS.escape(heading.attrs?.id ?? "")}"]`)
          ?.scrollIntoView({ block: "center" })}>{textOf(heading)}</Button
    >{/each}
  <hr style="border:0;border-top:1px solid var(--line);width:100%" />
  <h3>Writing is the starting point.</h3>
  <p class="muted">Select a passage to rewrite with AI or set how it should be spoken.</p>
  <Button
    onclick={() => {
      tab = "narrate";
      panel = "delivery";
    }}>Open narration</Button
  >
  <hr style="border:0;border-top:1px solid var(--line);width:100%" />
  <span class="caption muted">SCRIPT DETAILS</span>
  <p class="caption muted">
    {countWords(w.editor.document)} words<br />{w.editor.document.content.filter(
      (block) => block.type === "heading",
    ).length} sections<br />Saved on this device
  </p>
{/snippet}

{#if script}
  <div class="editor-toolbar">
    <Button
      variant="ghost"
      class="icon"
      aria-label="Back to library"
      onclick={() =>
        w.action(() => w.navigate({ kind: "library", view: "all", collectionId: null }))}
      ><ChevronLeft size={18} /></Button
    ><Button variant={tab === "write" ? "secondary" : "ghost"} onclick={() => (tab = "write")}
      ><PenLine size={15} />Write</Button
    ><Button
      variant={tab === "narrate" ? "secondary" : "ghost"}
      onclick={() =>
        w.action(async () => {
          await w.editor.flush();
          tab = "narrate";
          panel = "delivery";
        })}><Mic size={15} />Narrate</Button
    ><span class="push"></span><Button
      variant="ghost"
      class="icon"
      aria-label="Favorite this script"
      onclick={() => w.editor.metadata({ favorite: !script.favorite })}
      ><Star size={17} fill={script.favorite ? "currentColor" : "none"} /></Button
    ><Button
      variant="ghost"
      aria-label="Open delivery panel"
      onclick={() => {
        panel = "delivery";
        sheet = window.innerWidth < 1024;
      }}><SlidersHorizontal size={16} /><span class="desktop-only">Delivery</span></Button
    ><Button
      variant="ghost"
      aria-label="Open AI panel"
      onclick={() => ask({ text: "", anchor: null, range: null })}><Sparkles size={16} /></Button
    ><Button variant="ghost" class="icon" aria-label="Script actions" onclick={() => (menu = true)}
      ><MoreHorizontal size={18} /></Button
    >
  </div>
  {#if w.editor.error}<div class="inline-error" role="alert">
      <p>{w.editor.error} Your current words are still in this editor.</p>
      <div class="row">
        <Button onclick={() => w.action(() => w.editor.flush())}>Retry save</Button><Button
          onclick={() => w.action(recoveryCopy)}>Save local work as a new script</Button
        ><Button
          onclick={() =>
            w.action(async () => {
              const stored = await api.scripts.get(script.id);
              w.editor.load(stored);
            })}>Reload stored version</Button
        >
      </div>
    </div>{/if}
  <div class="editor-workspace">
    <div class="writing-scroll" hidden={tab !== "write"}>
      <div class="writing-canvas">
        <div class="script-meta">
          <select
            aria-label="Script collection"
            value={script.collectionId}
            onchange={(event) => w.editor.metadata({ collectionId: event.currentTarget.value })}
            >{#each w.collections as collection (collection.id)}<option value={collection.id}
                >{collection.name}</option
              >{/each}</select
          ><span>Private script</span>
        </div>
        <input
          class="title-input"
          aria-label="Script title"
          value={script.title}
          oninput={(event) => w.editor.title(event.currentTarget.value)}
        />{#key `${script.id}:${w.editor.documentEpoch}`}{#await editorComponent then component}<component.default
              bind:this={editorView}
              document={w.editor.document}
              onchange={(value) => w.editor.changeDocument(value)}
              onask={ask}
              onselection={(value) => (hasSelection = value)}
            />{/await}{/key}
      </div>
    </div>
    {#if tab === "narrate"}<div class="passage-list">
        <div class="page-heading">
          <div>
            <h1>{script.title}</h1>
            <p>Listen to the rhythm of your words.</p>
          </div>
        </div>
        <div class="toolbar">
          {#if render && foreignPlayback}<Button
              onclick={() =>
                w.action(async () => {
                  await api.playback.load(render.id);
                  await w.poll();
                })}>Load this narration</Button
            >{/if}
          <Button
            variant="primary"
            disabled={!script.session.voiceId || !!rendering}
            onclick={() => w.action(() => startRender())}><Mic size={16} />Render narration</Button
          >{#if rendering}<Button
              onclick={() =>
                renderJob &&
                w.action(async () => {
                  await api.jobs.cancel(renderJob.id);
                })}>Cancel</Button
            >{/if}<Badge tone="ready">On-device synthesis</Badge>
        </div>
        {#if rendering}<p class="muted">{renderJob?.phase}</p>{/if}{#if renderJob?.error}<p
            class="inline-error"
          >
            {renderJob.error.message}
          </p>{/if}{#if render}<div class="row">
            <Badge tone={render.revision === script.revision ? "ready" : "warning"}
              >{render.revision === script.revision
                ? "Ready to listen"
                : "Audio from an earlier revision"}</Badge
            ><select
              aria-label="Saved narration"
              value={render.id}
              onchange={(event) => {
                const id = event.currentTarget.value;
                void w.action(async () => {
                  chosenRender = id;
                  await api.playback.load(id);
                  await w.poll();
                });
              }}
              >{#if render.preview}<option value={render.id}>Passage preview</option
                >{/if}{#each w.renders.filter((r) => !r.preview) as audio (audio.id)}<option
                  value={audio.id}
                  >Revision {audio.revision} · {new Date(audio.createdAt).toLocaleString()}</option
                >{/each}</select
            >
          </div>{/if}{#each paragraphs as block, index (block.attrs?.id ?? index)}<article
            class="passage"
            class:active={w.playback?.renderId === render?.id &&
              !!block.attrs?.id &&
              !!w.playback?.currentBlockIds.includes(block.attrs.id)}
          >
            <div class="passage-heading">
              <Badge>{block.type === "pause" ? "Pause" : `Passage ${index + 1}`}</Badge
              >{#if block.attrs?.flowGroupId}<Badge>Continuous</Badge>{/if}<Button
                variant="ghost"
                disabled={!!rendering || !script.session.voiceId || block.type === "pause"}
                onclick={() =>
                  w.action(() => startRender(true, block.attrs?.id ? [block.attrs.id] : []))}
                >Preview</Button
              ><span class="caption muted"
                >{block.attrs?.delivery?.speed
                  ? `${block.attrs.delivery.speed}×`
                  : `${script.session.speed}×`}</span
              ><Button
                variant="ghost"
                onclick={() => {
                  tab = "write";
                  panel = "delivery";
                  sheet = window.innerWidth < 1024;
                }}>Edit delivery</Button
              >
            </div>
            <p>{textOf(block)}</p>
          </article>{/each}{#if !render}<p class="muted">
            Render a narration to listen or export. Your previous successful audio stays available
            when you make changes.
          </p>{/if}
      </div>{/if}
    <aside
      class="inspector"
      aria-label={panel === "ai"
        ? "AI assistant"
        : panel === "outline"
          ? "Page outline"
          : "Delivery inspector"}
    >
      {#if panel === "ai"}{@render ai()}{:else if panel === "outline"}{@render outline()}{:else}{@render delivery()}{/if}
    </aside>
  </div>
  <div class="editor-status">
    <span>{countWords(w.editor.document)} words · Revision {script.revision}</span><span
      aria-live="polite">{w.editor.status}</span
    >
  </div>
  {#if (tab === "narrate" || render) && !foreignPlayback}<StickyPlayer
      state={w.playback}
      {render}
      onplay={() => w.action(play)}
      oncontrol={(operation, value) =>
        w.action(async () => {
          if (operation === "volume" && value !== undefined)
            w.editor.session({ narrationVolume: value });
          await api.playback.control(operation, value ?? null);
          await w.poll();
        })}
      onexport={() => (exportOpen = true)}
    />{/if}
{/if}
<Dialog bind:open={sheet} title={panel === "ai" ? "Write with AI" : "Delivery"}
  >{#if panel === "ai"}{@render ai()}{:else}{@render delivery()}{/if}<Button
    onclick={() => (sheet = false)}>Done</Button
  ></Dialog
>
<Dialog bind:open={menu} title="Script actions"
  ><Button
    onclick={() =>
      w.action(async () => {
        if (!script) return;
        await w.editor.flush();
        const copy = await api.scripts.duplicate(script.id);
        menu = false;
        await w.navigate({ kind: "script", id: copy.id });
      })}><Copy size={16} />Duplicate</Button
  ><Button
    onclick={() =>
      w.action(async () => {
        if (script) await w.exportScript(script.id, script.title);
      })}><Download size={16} />Export script</Button
  ><Button
    onclick={() =>
      w.action(async () => {
        if (script) {
          revisions = await api.scripts.revisions(script.id);
          menu = false;
          revisionOpen = true;
        }
      })}><History size={16} />Revision history</Button
  ><Button
    variant="danger"
    onclick={() =>
      w.action(async () => {
        if (script) {
          await w.bulk([script.id], "trash");
          menu = false;
          await w.navigate({ kind: "library", view: "all", collectionId: null });
        }
      })}><Trash2 size={16} />Move to Trash</Button
  ><Button variant="ghost" onclick={() => (menu = false)}>Close</Button></Dialog
>
<Dialog
  bind:open={revisionOpen}
  title="Revision history"
  description="Restoring creates a new revision. Your history is kept."
  >{#each revisions as revision (revision.revision)}<div class="setting-row">
      <div>
        <h3>Revision {revision.revision} · {revision.title}</h3>
        <p>{new Date(revision.savedAt).toLocaleString()}</p>
      </div>
      <Button
        onclick={() =>
          w.action(async () => {
            if (!script) return;
            await w.editor.flush();
            const restored = await api.scripts.restoreRevision(script.id, revision.revision);
            w.editor.load(restored);
            revisionOpen = false;
            tab = "narrate";
          })}>Restore</Button
      >
    </div>{/each}<Button onclick={() => (revisionOpen = false)}>Done</Button></Dialog
>
<Dialog
  bind:open={exportOpen}
  title="Export WAV"
  description="48 kHz, 16-bit PCM. Exported files belong to you and are never automatically cleaned up."
  ><Field label="Include"
    ><select aria-label="Audio export content" bind:value={exportMixed}
      ><option value={false}>Narration only · mono</option><option
        value={true}
        disabled={!ambienceId}>Narration + ambience · stereo</option
      ></select
    ></Field
  >
  <div class="actions">
    <Button onclick={() => (exportOpen = false)}>Cancel</Button><Button
      variant="primary"
      disabled={!render}
      onclick={() =>
        w.action(async () => {
          if (!render || !script) return;
          const path = await chooseExport(script.title, true);
          if (path) {
            await api.narration.export(render.id, path, exportMixed ? ambienceId : null);
            exportOpen = false;
            w.notify("WAV export started");
          }
        })}>Export WAV</Button
    >
  </div></Dialog
>
