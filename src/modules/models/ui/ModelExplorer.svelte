<script lang="ts">
  import { Search, ArrowUpRight, Download, Heart, FileText, X } from "@lucide/svelte";
  import { Button, Badge, Input, Select, Dialog } from "../../../shared/ui";
  import { api } from "../../../shared/rpc/client";
  import ModelReadme from "./ModelReadme.svelte";
  import type { HubModel, HubDetail, HubSearch } from "../../../shared/rpc/generated/bindings";

  let { oninstall }: { oninstall: (catalogId: string) => Promise<void> } = $props();
  let query = $state("");
  let task = $state("text-to-speech");
  let sort = $state("downloads");
  let models = $state<HubModel[]>([]);
  let next = $state<string | null>(null);
  let loading = $state(true);
  let error = $state("");
  let detail = $state<HubDetail | null>(null);
  let detailOpen = $state(false);
  let detailLoading = $state(false);
  let detailError = $state("");
  let detailTab = $state("card");
  let fileQuery = $state("");
  let sequence = 0;
  let detailSequence = 0;
  let detailRepository = $state("");
  let linkError = $state("");
  const files = $derived(
    detail?.files.filter((file) => file.path.toLowerCase().includes(fileQuery.toLowerCase())) ?? [],
  );

  $effect(() => {
    const request: HubSearch = { query, task, sort, cursor: null };
    sequence++;
    models = [];
    next = null;
    const timer = setTimeout(() => void search(false, request), 350);
    return () => clearTimeout(timer);
  });

  async function search(
    append: boolean,
    request: HubSearch = { query, task, sort, cursor: append ? next : null },
  ): Promise<void> {
    const current = ++sequence;
    loading = true;
    error = "";
    try {
      const result = await api.models.search(request);
      if (current !== sequence) return;
      models = append
        ? [...models, ...result.models].filter(
            (row, index, all) =>
              all.findIndex((other) => other.repository === row.repository) === index,
          )
        : result.models;
      next = result.nextCursor;
    } catch (value) {
      if (current === sequence)
        error = value instanceof Error ? value.message : "The model search failed.";
    } finally {
      if (current === sequence) loading = false;
    }
  }

  async function inspect(repository: string): Promise<void> {
    detailRepository = repository;
    const current = ++detailSequence;
    detail = null;
    detailLoading = true;
    detailOpen = true;
    detailError = "";
    detailTab = "card";
    fileQuery = "";
    try {
      const result = await api.models.detail(repository);
      if (current === detailSequence) detail = result;
    } catch (value) {
      if (current === detailSequence)
        detailError = value instanceof Error ? value.message : "Model details are unavailable.";
    } finally {
      if (current === detailSequence) detailLoading = false;
    }
  }

  async function openSource(url: string): Promise<void> {
    linkError = "";
    try {
      await api.models.openSource(url);
    } catch (value) {
      linkError = value instanceof Error ? value.message : "The source page could not be opened.";
    }
  }

  function count(value: string): string {
    const amount = Number(value);
    return Number.isFinite(amount)
      ? new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 }).format(
          amount,
        )
      : value;
  }

  function size(value: string | null): string {
    if (value === null) return "Size not supplied";
    const n = Number(value);
    return n >= 1e9
      ? `${(n / 1e9).toFixed(2)} GB`
      : n >= 1e6
        ? `${(n / 1e6).toFixed(1)} MB`
        : `${Math.ceil(n / 1000)} KB`;
  }

  function support(model: HubModel): string {
    return model.compatibility.kind === "catalog"
      ? "Supported download"
      : model.compatibility.kind === "candidate"
        ? "GGUF · verify first"
        : "Browse only";
  }
</script>

<section class="model-explorer" aria-label="Model explorer">
  <p class="muted">
    Search public Hugging Face models. You can research any model; compatibility and installation
    are shown separately.
  </p>
  <div class="hub-search">
    <div class="search-field">
      <Search size={16} /><Input
        aria-label="Search Hugging Face models"
        bind:value={query}
        placeholder="Search a model, family or author…"
      />
    </div>
    <Select
      label="Model task"
      bind:value={task}
      options={[
        { value: "all", label: "All model tasks" },
        { value: "text-generation", label: "Text generation" },
        { value: "text-to-speech", label: "Text to speech" },
      ]}
    /><Select
      label="Sort models"
      bind:value={sort}
      options={[
        { value: "downloads", label: "Most downloaded" },
        { value: "likes", label: "Most liked" },
        { value: "lastModified", label: "Recently updated" },
      ]}
    />
  </div>
  <div class="hub-suggestions">
    <span class="caption muted">Try</span
    >{#each ["Qwen3-TTS", "Chatterbox", "Kokoro", "Supertonic"] as name (name)}<Button
        variant="ghost"
        onclick={() => {
          query = name;
          task = "text-to-speech";
        }}>{name}</Button
      >{/each}
  </div>
  {#if linkError && !detailOpen}<p class="inline-error" role="alert">{linkError}</p>{/if}
  {#if error}<div class="inline-error" role="alert">{error}</div>
    <Button onclick={() => search(false)}>Retry search</Button>{/if}
  <div class="hub-results" aria-busy={loading}>
    {#each models as model (model.repository)}<article class="card hub-model">
        <div class="row spread">
          <Badge tone={model.compatibility.kind === "catalog" ? "ready" : "neutral"}
            >{support(model)}</Badge
          >{#if model.gated}<Badge tone="warning">Gated model</Badge>{/if}
        </div>
        <h2>
          <button class="model-title" onclick={() => inspect(model.repository)}
            >{model.repository}</button
          >
        </h2>
        <p class="caption muted">
          {model.task ?? "Task not specified"}{model.library ? ` · ${model.library}` : ""}
        </p>
        <div class="hub-stats">
          <span><Download size={13} />{count(model.downloads)}</span><span
            ><Heart size={13} />{count(model.likes)}</span
          ><span>{model.license ?? "License not supplied"}</span>
        </div>
        <div class="row spread">
          <Button onclick={() => inspect(model.repository)}><FileText size={15} />View model</Button
          ><a
            href={`https://huggingface.co/${model.repository}`}
            onclick={(event) => {
              event.preventDefault();
              void openSource(event.currentTarget.href);
            }}
            target="_blank"
            rel="noreferrer"
            aria-label={`Open ${model.repository} on Hugging Face`}><ArrowUpRight size={17} /></a
          >
        </div>
      </article>{/each}
  </div>
  {#if loading}<p class="muted" role="status">
      Searching the model hub…
    </p>{:else if !models.length && !error}<div class="empty-state">
      <Search size={28} />
      <h2>No matching models</h2>
      <p>Try another name or broaden the task filter.</p>
    </div>{/if}
  {#if next}<Button disabled={loading} onclick={() => search(true)}>Load more models</Button>{/if}
</section>
<Dialog bind:open={detailOpen} title="Model details" class="hub-detail-modal">
  <Button
    variant="ghost"
    class="icon hub-detail-close"
    aria-label="Close model details"
    onclick={() => (detailOpen = false)}><X size={18} /></Button
  >
  {#if detailLoading}<p class="muted" role="status">
      Loading model card and files…
    </p>{:else if detailError}<p class="inline-error" role="alert">{detailError}</p>
    <Button onclick={() => inspect(detailRepository)}>Retry details</Button>{:else if detail}<div
      class="hub-details"
    >
      <header>
        <h2>{detail.model.repository}</h2>
        <div class="row">
          <Badge tone={detail.model.compatibility.kind === "catalog" ? "ready" : "neutral"}
            >{support(detail.model)}</Badge
          ><span class="caption muted"
            >{detail.model.task ?? "Unknown task"} · {detail.model.license ??
              "Unknown license"}</span
          >
        </div>
        {#if detail.model.compatibility.kind !== "catalog"}<p class="hub-compatibility">
            {detail.model.compatibility.reason}
          </p>{/if}
      </header>
      <div class="toolbar">
        <Button aria-pressed={detailTab === "card"} onclick={() => (detailTab = "card")}
          >Model card</Button
        ><Button aria-pressed={detailTab === "files"} onclick={() => (detailTab = "files")}
          >Files ({detail.files.length}{detail.filesTruncated ? "+" : ""})</Button
        ><span class="push"></span><span class="caption muted"
          >{count(detail.model.downloads)} downloads · {detail.languages.join(", ") ||
            "Languages not specified"}</span
        >
      </div>
      <div class="hub-details-body">
        {#if detailTab === "card"}{#if detail.readme}<ModelReadme source={detail.readme} />{:else}<p
              class="muted"
            >
              {detail.description ??
                "No model-card text was returned. Open the source page for the full documentation."}
            </p>{/if}{:else}<Input
            aria-label="Filter model files"
            bind:value={fileQuery}
            placeholder="Filter files by name…"
          />
          <div class="hub-files">
            {#each files as file (file.path)}<a
                href={`https://huggingface.co/${detail.model.repository}/blob/${encodeURIComponent(detail.revision || "main")}/${file.path.split("/").map(encodeURIComponent).join("/")}`}
                onclick={(event) => {
                  event.preventDefault();
                  void openSource(event.currentTarget.href);
                }}
                target="_blank"
                rel="noreferrer"
                ><span>{file.path}</span><span class="caption muted">{size(file.bytes)}</span></a
              >{/each}
          </div>
          {#if detail.filesTruncated}<p class="caption muted">
              Showing the first 1,000 files. Open the source page to browse the complete repository.
            </p>{/if}{/if}
      </div>
      {#if linkError}<p class="inline-error" role="alert">{linkError}</p>{/if}
      <footer class="row spread">
        <a
          href={`https://huggingface.co/${detail.model.repository}`}
          onclick={(event) => {
            event.preventDefault();
            void openSource(event.currentTarget.href);
          }}
          target="_blank"
          rel="noreferrer">Full model page ↗</a
        >{#if detail.model.compatibility.kind === "catalog"}<Button
            variant="primary"
            onclick={() => {
              if (detail?.model.compatibility.kind === "catalog") {
                detailOpen = false;
                void oninstall(detail.model.compatibility.catalogId);
              }
            }}>Review supported download</Button
          >{:else}<Button onclick={() => (detailOpen = false)}>Done</Button>{/if}
      </footer>
    </div>{/if}
</Dialog>
