<script lang="ts">
  import { onMount } from "svelte";
  import {
    Box,
    Download,
    FolderOpen,
    Link2,
    Cpu,
    Mic,
    Check,
    RefreshCw,
    Trash2,
    ShieldCheck,
  } from "@lucide/svelte";
  import { Button, Badge, Dialog, Field, Select } from "../shared/ui";
  import { api } from "../shared/rpc/client";
  import { newJobId } from "../domain/ids";
  import { chooseGguf, chooseModelFolder } from "../shared/platform/dialogs";
  import type { Workspace } from "../app/workflows/workspace.svelte";
  import type { CatalogEntry, InstallPlan, ModelRecord, ScriptSummary } from "../domain";

  let { workspace: w }: { workspace: Workspace } = $props();
  let tab = $state("installed");
  let catalog = $state<CatalogEntry[]>([]);
  let plan = $state<InstallPlan | null>(null);
  let installOpen = $state(false);
  let remove = $state<ModelRecord | null>(null);
  let removeOpen = $state(false);
  let usage = $state<ScriptSummary[]>([]);
  let linkOpen = $state(false);
  let kind = $state("text");
  let linking = $state(false);
  let relinkId = $state<string | null>(null);

  onMount(() => {
    void w.action(async () => {
      catalog = await api.models.catalog();
    });
  });

  function bytes(value: string): string {
    const size = Number(value);
    return size >= 1e9 ? `${(size / 1e9).toFixed(2)} GB` : `${(size / 1e6).toFixed(1)} MB`;
  }

  async function link(): Promise<void> {
    linking = true;
    try {
      const path = kind === "text" ? await chooseGguf() : await chooseModelFolder();
      if (path) {
        const model = await api.models.link(path, kind === "text" ? null : kind, relinkId);
        await api.models.verify(model.id, newJobId());
        linkOpen = false;
        await w.refresh();
      }
    } finally {
      linking = false;
    }
  }
</script>

<div class="page">
  <header class="page-heading">
    <div>
      <h1>Models & voices</h1>
      <p>The tools behind your words. Run them on your device.</p>
    </div>
    <Button variant="primary" onclick={() => (tab = "catalog")}
      ><Download size={16} />Browse models</Button
    >
  </header>
  <div class="toolbar">
    <Button
      variant={tab === "installed" ? "secondary" : "ghost"}
      onclick={() => (tab = "installed")}>Installed models</Button
    ><Button variant={tab === "catalog" ? "secondary" : "ghost"} onclick={() => (tab = "catalog")}
      >Model catalog</Button
    ><span class="push"></span><Button
      onclick={() => {
        relinkId = null;
        linkOpen = true;
      }}><Link2 size={15} />Link existing</Button
    ><Button
      class="icon"
      aria-label="Rescan model folder"
      onclick={() =>
        w.action(async () => {
          await api.models.scan();
          await w.refresh();
        })}><RefreshCw size={16} /></Button
    >
  </div>
  {#if tab === "installed"}
    {#if w.models.length}<div class="card-grid">
        {#each w.models as model (model.id)}<article class="card">
            <div class="card-title">
              {#if model.capability === "speech"}<Mic size={22} />{:else}<Cpu size={22} />{/if}
              <h2>{model.name}</h2>
              <Badge
                tone={model.status === "ready"
                  ? "ready"
                  : model.status === "missing" || model.status === "invalid"
                    ? "error"
                    : "warning"}
                >{model.status === "ready"
                  ? "Ready"
                  : model.status === "available"
                    ? "Needs verification"
                    : model.status}</Badge
              >
            </div>
            <p class="muted">
              {model.provider === "openrouter"
                ? "Cloud text · OpenRouter"
                : model.capability === "speech"
                  ? `${model.architecture} · ${model.voices.length} voices · Local speech`
                  : "GGUF · Local text"}
            </p>
            {#if model.reason}<p class="inline-error">{model.reason}</p>{/if}
            <div class="caption muted" style="overflow-wrap:anywhere">
              {model.ownership === "managed"
                ? "Managed download"
                : model.ownership === "linked"
                  ? "Linked · stays in its original folder"
                  : "Remote model"}{#if model.provider !== "openrouter"}<br />{model.path}{/if}
            </div>
            <div class="row">
              {#if model.capability === "text"}<Button
                  disabled={model.status !== "ready"}
                  variant={w.preferences?.textModelId === model.id ? "ghost" : "secondary"}
                  onclick={() => w.action(() => w.savePreferences({ textModelId: model.id }))}
                  >{#if w.preferences?.textModelId === model.id}<Check size={15} />Default for
                    writing{:else}Use for writing{/if}</Button
                >{:else}<Select
                  label={`Default voice from ${model.name}`}
                  disabled={model.status !== "ready"}
                  value={model.voices.some((v) => v.id === w.preferences?.voiceId)
                    ? (w.preferences?.voiceId ?? "")
                    : ""}
                  style="width:260px"
                  onchange={(value) => w.action(() => w.savePreferences({ voiceId: value || null }))}
                  options={[
                    { value: "", label: `Set default voice…` },
                    ...model.voices.map((voice) => ({
                      value: voice.id,
                      label: `${voice.name} · ${voice.language}`,
                    })),
                  ]}
                />{/if}
              {#if model.provider !== "openrouter"}<Button
                  variant="ghost"
                  onclick={() =>
                    w.action(async () => {
                      await api.models.verify(model.id, newJobId());
                      await w.poll();
                    })}><ShieldCheck size={15} />Verify</Button
                ><Button
                  variant="ghost"
                  onclick={() => {
                    relinkId = model.id;
                    kind = model.architecture ?? "text";
                    linkOpen = true;
                  }}>Relink</Button
                >{/if}
              <Button
                variant="ghost"
                class="icon"
                aria-label={`Remove ${model.name}`}
                onclick={() =>
                  w.action(async () => {
                    usage = await api.models.usage(model.id);
                    remove = model;
                    removeOpen = true;
                  })}><Trash2 size={16} /></Button
              >
            </div>
          </article>{/each}
      </div>{:else}<div class="empty-state">
        <Box size={34} />
        <h2>Your local toolkit starts here</h2>
        <p>Install a writing model and a voice, or link files you already have.</p>
        <Button variant="primary" onclick={() => (tab = "catalog")}>Explore models</Button>
      </div>{/if}
  {:else}<p class="muted">
      Downloads go to your chosen model folder. Review the source, license and size before
      installing.
    </p>
    <div class="card-grid">
      {#each catalog as item (item.id)}<article class="card">
          <div class="card-title">
            {#if item.capability === "speech"}<Mic size={22} />{:else}<Cpu size={22} />{/if}
            <h2>{item.name}</h2>
            <Badge>{item.capability}</Badge>
          </div>
          <p class="muted">{item.description}</p>
          <p class="caption muted">{item.repository}</p>
          <div class="row spread">
            <a href={`https://huggingface.co/${item.repository}`} target="_blank" rel="noreferrer"
              >Model source ↗</a
            ><Button
              variant="primary"
              disabled={w.busy}
              onclick={() =>
                w.action(async () => {
                  plan = await api.models.plan(item.id);
                  installOpen = true;
                })}><Download size={16} />Review download</Button
            >
          </div>
        </article>{/each}
    </div>{/if}
  <div class="hint">
    <FolderOpen size={18} /><span style="overflow-wrap:anywhere"
      >Model folder<br /><strong>{w.preferences?.modelRoot ?? "Loading…"}</strong></span
    ><Button
      variant="ghost"
      onclick={() => w.action(() => w.navigate({ kind: "settings", section: "storage" }))}
      >Change</Button
    >
  </div>
</div>
<Dialog
  bind:open={linkOpen}
  title={relinkId ? "Relink model" : "Link an existing model"}
  description="Your files stay where they are. Unlinking never deletes them."
  ><Field label="Model type"
    ><Select
      label="Model type"
      bind:value={kind}
      options={[
        { value: "text", label: `Text · GGUF file` },
        { value: "kokoro", label: `Kokoro · speech folder` },
        { value: "supertonic", label: `Supertonic · speech folder` },
      ]}
    /></Field
  >
  <p class="muted">
    Speech folders must include the model, configuration and voice files expected by NobodyWho.
  </p>
  <div class="actions">
    <Button onclick={() => (linkOpen = false)}>Cancel</Button><Button
      variant="primary"
      loading={linking}
      onclick={() => w.action(link)}>Choose {kind === "text" ? "file" : "folder"}</Button
    >
  </div></Dialog
>
<Dialog
  bind:open={installOpen}
  title="Install model"
  description="The download is verified before it becomes available."
  >{#if plan}<h2>{plan.catalog.name}</h2>
    <dl class="caption">
      <dt>Source</dt>
      <dd>{plan.catalog.repository}</dd>
      <dt>Revision</dt>
      <dd style="overflow-wrap:anywhere">{plan.revision}</dd>
      <dt>Download</dt>
      <dd>{bytes(plan.totalBytes)} · {plan.files.length} files</dd>
      <dt>License</dt>
      <dd>{plan.license}</dd>
      <dt>Destination</dt>
      <dd style="overflow-wrap:anywhere">{plan.destination}</dd>
    </dl>
    <div class="actions">
      <Button onclick={() => (installOpen = false)}>Cancel</Button><Button
        variant="primary"
        onclick={() =>
          w.action(async () => {
            if (!plan) return;
            await api.models.install(plan.id, newJobId());
            installOpen = false;
            tab = "installed";
            await w.poll();
          })}>Install model</Button
      >
    </div>{/if}</Dialog
>
<Dialog
  bind:open={removeOpen}
  title={remove?.ownership === "managed" ? "Remove downloaded model?" : "Unlink model?"}
  description={remove?.ownership === "managed"
    ? "The managed model files will be deleted. Scripts and saved audio stay available. Any affected default will be cleared."
    : "The registration and affected defaults will be removed. Your original model files, scripts and saved audio remain."}
  >{#if usage.length}<p class="muted">
      These scripts reference this model. Choose a replacement voice or model before rendering them
      again.
    </p>
    {#each usage as script (script.id)}<p class="caption">
        {script.title} · {script.collectionName}
      </p>{/each}{/if}
  <div class="actions">
    <Button onclick={() => (removeOpen = false)}>Cancel</Button><Button
      variant="danger"
      onclick={() =>
        w.action(async () => {
          if (remove) await api.models.remove(remove.id);
          removeOpen = false;
          w.preferences = await api.settings.get();
          await w.refresh();
        })}>{remove?.ownership === "managed" ? "Remove download" : "Unlink"}</Button
    >
  </div></Dialog
>
