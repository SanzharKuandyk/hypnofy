<script lang="ts">
  import { onMount, tick } from "svelte";
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
    Music2,
  } from "@lucide/svelte";
  import { Button, Badge, Dialog, Field, Select, Input, Slider } from "../shared/ui";
  import { loadScriptEditor } from "../modules/editor";
  import type { EditorSelection } from "../modules/editor";
  import { DeliveryPanel, NarrationText, RecordingDeleteDialog } from "../modules/narration";
  import { GenerationPanel } from "../modules/generation";
  import { StickyPlayer } from "../modules/playback";
  import { api } from "../shared/rpc/client";
  import { chooseAmbience, chooseExport } from "../shared/platform/dialogs";
  import { countWords, textOf } from "../domain/document";
  import { newJobId } from "../domain/ids";
  import type { DeliveryOverride } from "../domain/document";
  import type { Workspace } from "../app/workflows/workspace.svelte";
  import type { Suggestion, RevisionRecord } from "../domain";

  interface EditorActions {
    applySuggestion(text: string, replace: boolean, suggestion: Suggestion): void;
    applyDelivery(value: DeliveryOverride): void;
    makeContinuous(): void;
    focusPassage(id: string): void;
    requestAi(): void;
  }

  let { workspace: w }: { workspace: Workspace } = $props();
  const editorComponent = loadScriptEditor();
  let editorView = $state<EditorActions>();
  let titleInput = $state<HTMLInputElement>();
  let tab = $state("write");
  let soundOpen = $state(false);
  let soundId = $state("");
  let soundVolume = $state(0);
  let soundSaving = $state(false);
  let soundError = $state("");
  let playerClosed = $state(false);
  let recordingDeleteOpen = $state(false);
  let recordingDeleteId = $state<string | null>(null);
  let deliveryOpen = $state(false);
  let aiOpen = $state(false);
  let menu = $state(false);
  let revisions = $state<RevisionRecord[]>([]);
  let revisionOpen = $state(false);
  let selectedText = $state("");
  let selection = $state<EditorSelection>({ text: "", anchor: null, range: null });
  let hasSelection = $state(false);
  let writingModel = $state("");
  let generationSequence = $state(0);
  let generationJobId = $state("");
  let renderJobId = $state("");
  let exportOpen = $state(false);
  let exportMixed = $state(false);
  let chosenRender = $state("");
  const script = $derived(w.editor.script);
  const ambienceId = $derived(script?.session.ambienceId ?? null);
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
      w.renders.some((r) => r.id === id)
    ) {
      playedPreview = id;
      chosenRender = id;
      playerClosed = false;
      void w.action(async () => {
        await api.playback.load(id);
        if (w.renders.some((r) => r.id === id && r.preview)) await api.playback.control("play");
        await w.poll();
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
    if (w.route.kind === "script" && w.route.tab === "narrate") tab = "narrate";
    else if (!script?.title.trim() || script.title === "Untitled script") focusTitle();
  });

  function focusTitle(): void {
    titleInput?.focus();
    titleInput?.select();
  }

  function openSound(): void {
    if (!script) return;
    soundId = script.session.ambienceId ?? "";
    soundVolume = script.session.ambienceVolume ?? w.preferences?.ambienceVolume ?? 0.06;
    soundError = "";
    soundOpen = true;
  }

  async function importSound(): Promise<void> {
    soundError = "";
    soundSaving = true;
    try {
      const path = await chooseAmbience();
      if (path) {
        const sound = await api.playback.importAmbience(path);
        w.ambience = [...w.ambience, sound];
        soundId = sound.id;
      }
    } catch (value) {
      soundError = value instanceof Error ? value.message : "Could not import audio.";
    } finally {
      soundSaving = false;
    }
  }

  async function saveSound(): Promise<void> {
    if (!script) return;
    const ambienceId = soundId || null;
    const ambienceVolume = soundVolume;
    soundError = "";
    soundSaving = true;
    try {
      w.editor.session({ ambienceId, ambienceVolume });
      await w.editor.flush();
      soundOpen = false;
      if (w.renders.some((item) => item.id === w.playback?.renderId)) {
        await w.action(async () => {
          await api.playback.control("ambience-volume", ambienceVolume);
          await api.playback.selectAmbience(ambienceId);
          await w.poll();
        });
      }
    } catch (value) {
      soundError = value instanceof Error ? value.message : "Could not save background sound.";
    } finally {
      soundSaving = false;
    }
  }

  function ask(snapshot: EditorSelection): void {
    selection = snapshot;
    selectedText = snapshot.text;
    aiOpen = true;
    generationSequence = w.editor.editSequence;
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
      aiOpen = false;
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
    playerClosed = false;
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
      aiOpen = false;
    }}
    {stale}
    error={w.error}
  />
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
        })}><Mic size={15} />Narrate</Button
    ><span class="push"></span><Button
      variant="ghost"
      aria-label="Favorite this script"
      class="icon script-favorite"
      onclick={() => w.editor.metadata({ favorite: !script.favorite })}
      ><Star size={17} fill={script.favorite ? "currentColor" : "none"} /></Button
    ><Button
      variant="ghost"
      aria-label="Open delivery panel"
      onclick={() => {
        deliveryOpen = true;
      }}><SlidersHorizontal size={16} /><span class="desktop-only">Voice & timing</span></Button
    ><Button variant="ghost" aria-label="Open AI panel" onclick={() => editorView?.requestAi()}
      ><Sparkles size={16} /><span class="desktop-only">Write with AI</span></Button
    ><Button variant="ghost" aria-label="Background sound" class="script-sounds" onclick={openSound}
      ><Music2 size={16} /><span class="desktop-only">Sounds</span></Button
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
          <Select
            label="Script collection"
            value={script.collectionId}
            onchange={(value) => w.editor.metadata({ collectionId: value })}
            options={[
              ...w.collections.map((collection) => ({
                value: collection.id,
                label: `${collection.name}`,
              })),
            ]}
          /><span>Private script</span>
        </div>
        <Input
          class="title-input"
          bind:element={titleInput}
          aria-label="Script title"
          value={script.title}
          oninput={(value) => w.editor.title(value)}
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
          <Button
            onclick={() =>
              w.action(() =>
                w.navigate({ kind: "sounds", returnToScriptId: script.id, view: "recordings" }),
              )}>Manage recordings</Button
          >
          {#if playerClosed && render}<Button onclick={() => (playerClosed = false)}
              >Show player</Button
            >{/if}
          <Button onclick={() => (deliveryOpen = true)}>Voice & timing</Button><Button
            onclick={openSound}><Music2 size={15} />Background sound</Button
          >
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
            <Badge
              tone={render.revision === script.revision && render.pipelineVersion === 2
                ? "ready"
                : "warning"}
              >{render.pipelineVersion !== 2
                ? "Re-render to apply the updated pause pipeline"
                : render.revision === script.revision
                  ? "Ready to listen"
                  : "Audio from an earlier revision"}</Badge
            ><Select
              label="Saved narration"
              value={render.id}
              onchange={(value) => {
                const id = value;
                void w.action(async () => {
                  chosenRender = id;
                  await api.playback.load(id);
                  await w.poll();
                });
              }}
              options={[
                ...(render.preview ? [{ value: render.id, label: `Passage preview` }] : []),
                ...w.renders
                  .filter((r) => !r.preview)
                  .map((audio) => ({
                    value: audio.id,
                    label: `Revision ${audio.revision} · ${new Date(audio.createdAt).toLocaleString()}`,
                  })),
              ]}
            />
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
                  void tick().then(() => {
                    if (block.attrs?.id) editorView?.focusPassage(block.attrs.id);
                    deliveryOpen = true;
                  });
                }}>Edit delivery</Button
              >
            </div>
            <p><NarrationText text={textOf(block)} /></p>
          </article>{/each}{#if !render}<p class="muted">
            Render a narration to listen or export. Your previous successful audio stays available
            when you make changes.
          </p>{/if}
      </div>{/if}
  </div>
  <div class="editor-status">
    <span>{countWords(w.editor.document)} words · Revision {script.revision}</span><span
      aria-live="polite">{w.editor.status}</span
    >
  </div>
  {#if tab === "write" && render && render.pipelineVersion !== 2}<div class="render-warning">
      <span>This audio uses the older pause pipeline. Re-render to hear the updated timing.</span
      ><Button variant="ghost" onclick={() => (tab = "narrate")}>Open narration</Button>
    </div>{/if}
  {#if (tab === "narrate" || render) && !foreignPlayback && !playerClosed}<StickyPlayer
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
      onclose={() =>
        w.action(async () => {
          playerClosed = true;
          await api.playback.control("unload");
          await w.refreshAudio();
        })}
      ondelete={() => {
        recordingDeleteId = render?.id ?? null;
        recordingDeleteOpen = true;
      }}
    />{/if}
{/if}
<RecordingDeleteDialog
  id={recordingDeleteId}
  bind:open={recordingDeleteOpen}
  onchanged={() => w.refreshAudio()}
  onremoved={() => {
    playerClosed = true;
    chosenRender = "";
    w.notify("Recording deleted");
  }}
/>
<Dialog bind:open={aiOpen} title="Write with AI" class="ai-modal">{@render ai()}</Dialog>
<Dialog bind:open={soundOpen} title="Background sound" required={soundSaving}>
  <Field label="Sound">
    <Select
      label="Sound for this script"
      bind:value={soundId}
      disabled={soundSaving}
      options={[
        { value: "", label: "No background sound" },
        ...w.ambience.map((sound) => ({ value: sound.id, label: sound.name })),
      ]}
    />
  </Field>
  {#if soundId}
    <Field label={`Volume · ${Math.round(soundVolume * 100)}%`}>
      <Slider label="Background sound volume" bind:value={soundVolume} disabled={soundSaving} />
    </Field>
  {/if}
  {#if soundError}<p class="inline-error" role="alert">{soundError}</p>{/if}
  <div class="actions">
    <Button variant="ghost" disabled={soundSaving} onclick={importSound}>Import audio</Button>
    <Button disabled={soundSaving} onclick={() => (soundOpen = false)}>Cancel</Button>
    <Button variant="primary" loading={soundSaving} onclick={saveSound}>Save sound</Button>
  </div>
</Dialog>
<Dialog bind:open={deliveryOpen} title="Voice & timing" class="delivery-modal"
  >{#if script}<DeliveryPanel
      session={script.session}
      {voices}
      {hasSelection}
      onscript={(patch) => w.editor.session(patch)}
      onapply={(value) => editorView?.applyDelivery(value)}
      oncontinuous={() => editorView?.makeContinuous()}
      onclose={() => (deliveryOpen = false)}
    />{/if}</Dialog
>
<Dialog bind:open={menu} title="Script actions"
  ><Button
    onclick={() => {
      menu = false;
      openSound();
    }}><Music2 size={16} />Background sound</Button
  >
  <Button
    onclick={async () => {
      menu = false;
      tab = "write";
      await tick();
      focusTitle();
    }}>Rename script</Button
  >
  <Button
    onclick={() => {
      if (script) w.editor.metadata({ favorite: !script.favorite });
    }}><Star size={16} />Toggle favorite</Button
  >
  <Button
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
    ><Select
      label="Audio export content"
      bind:value={exportMixed}
      options={[
        { value: false, label: `Narration only · mono` },
        { value: true, label: `Narration + ambience · stereo`, disabled: !ambienceId },
      ]}
    /></Field
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
