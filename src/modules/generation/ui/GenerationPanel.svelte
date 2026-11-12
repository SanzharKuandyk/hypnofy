<script lang="ts">
  import {
    Sparkles,
    X,
    Copy,
    Trash2,
    WandSparkles,
    ChevronRight,
    CornerDownRight,
  } from "@lucide/svelte";
  import { Button, Field, Badge, Select, Textarea } from "../../../shared/ui";
  import { api } from "../../../shared/rpc/client";
  import { narrationParts } from "../../../domain/pause-cues";
  import type { ModelRecord, JobSnapshot, Suggestion } from "../../../domain";

  let {
    models,
    suggestions,
    job,
    selectedText,
    modelId = $bindable(""),
    onstart,
    onapply,
    ondiscard,
    onclose,
    baseRevision,
    baseContentHash,
    stale = false,
    error = "",
  }: {
    models: ModelRecord[];
    suggestions: Suggestion[];
    job: JobSnapshot | null;
    selectedText: string;
    modelId?: string;
    onstart: (prompt: string, operation: string) => Promise<void>;
    onapply: (suggestion: Suggestion, replace: boolean) => void;
    ondiscard: (id: string) => void;
    onclose: () => void;
    baseRevision: number;
    baseContentHash: string;
    stale?: boolean;
    error?: string;
  } = $props();

  let instruction = $state("");
  let operation = $derived(selectedText ? "rewrite-selection" : "continue");
  let view = $state<"compose" | "preview">("compose");
  let chosen = $state("");
  const running = $derived(!!job && ["queued", "running", "cancelling"].includes(job.state));
  const ready = $derived(
    models.filter((model) => model.capability === "text" && model.status === "ready"),
  );
  const current = $derived(suggestions.find((s) => s.id === chosen) ?? suggestions.at(-1));
  const invalid = $derived(
    stale || current?.baseRevision !== baseRevision || current?.baseContentHash !== baseContentHash,
  );
  const text = $derived(running ? (job?.text ?? "") : (current?.text ?? ""));
  const parts = $derived.by(() => {
    try {
      return narrationParts(text);
    } catch {
      return [{ kind: "speech" as const, text, from: 0, to: text.length }];
    }
  });

  async function generate(): Promise<void> {
    view = "preview";
    await onstart(instruction, operation);
  }
</script>

<div class="ai-workspace-top">
  <p class="muted">Shape a draft, read it, then decide what belongs in your script.</p>
  <Button variant="ghost" class="icon" aria-label="Close AI workspace" onclick={onclose}
    ><X size={18} /></Button
  >
</div>
<div class="workspace-tabs" aria-label="AI workspace views">
  <Button aria-pressed={view === "compose"} onclick={() => (view = "compose")}>Compose</Button>
  <Button aria-pressed={view === "preview"} onclick={() => (view = "preview")}>Preview</Button>
</div>
<div class="ai-workspace" data-view={view}>
  <section class="ai-composer" aria-label="AI composer">
    <Field label="Writing model"
      ><Select
        label="Writing model"
        bind:value={modelId}
        disabled={running}
        options={[
          { value: "", label: "Choose a model" },
          ...ready.map((model) => ({
            value: model.id,
            label: `${model.name} · ${model.provider === "openrouter" ? "Cloud" : "Local"}`,
          })),
        ]}
      /></Field
    >
    <Field label="What would you like to do?"
      ><Select
        label="Draft operation"
        bind:value={operation}
        disabled={running}
        options={[
          { value: "continue", label: "Continue script" },
          { value: "new", label: "New draft" },
          { value: "rewrite-selection", label: "Rewrite selection", disabled: !selectedText },
        ]}
      /></Field
    >
    <Field label="Your instruction"
      ><Textarea
        aria-label="AI instruction"
        bind:value={instruction}
        disabled={running}
        placeholder="Continue with a gentle breathing passage. Keep the language grounded and unhurried…"
      /></Field
    >
    {#if selectedText}<blockquote class="selection-excerpt">{selectedText}</blockquote>{/if}
    <p class="caption muted">
      {models.find((m) => m.id === modelId)?.provider === "openrouter"
        ? "This request sends the saved script context to OpenRouter."
        : "Uses the saved script as context. Local requests stay on this device."}
    </p>
    {#if error || job?.error}<p class="inline-error" role="alert">
        {error || job?.error?.message}
      </p>{/if}
    <div class="composer-actions">
      {#if running}<Button onclick={() => job && api.jobs.cancel(job.id)}>Cancel generation</Button>
        <p class="caption muted" role="status">{job?.phase || "Writing your suggestion…"}</p>
      {:else}<Button
          variant="primary"
          disabled={!ready.some((model) => model.id === modelId) || !instruction.trim()}
          onclick={generate}><WandSparkles size={16} />Generate suggestion</Button
        >{/if}
    </div>
  </section>
  <section class="ai-review" aria-label="Suggestion review">
    <div class="review-heading">
      <span class="eyebrow">{running ? "WRITING A SUGGESTION" : "SUGGESTED PASSAGE"}</span>
      {#if suggestions.length > 1 && !running}<Select
          label="Saved suggestion"
          value={current?.id ?? ""}
          onchange={(id) => {
            chosen = id;
          }}
          options={suggestions.map((s, index) => ({ value: s.id, label: `Draft ${index + 1}` }))}
        />{:else}<Badge
          >{running ? "Generating" : current ? "Ready for review" : "Your next draft"}</Badge
        >{/if}
    </div>
    <div class="draft-reading" aria-busy={running}>
      {#if text}<article class="draft-prose" aria-label="Suggested text">
          {#each parts as part (part.from)}{#if part.kind === "speech"}{part.text}{:else}<span
                class="pause-chip">◷ Pause · {part.durationMs / 1000}s</span
              >{/if}{/each}
        </article>
      {:else}<div class="draft-empty">
          <Sparkles size={28} />
          <h3>{running ? "Finding the next words…" : "Room for a new idea"}</h3>
          <p>
            {running
              ? "The preview will appear here as it is written."
              : "Describe what you want on the left. Nothing changes in your script until you insert a suggestion."}
          </p>
        </div>{/if}
    </div>
    <div class="review-actions">
      {#if current && !running}
        {#if invalid}<p class="inline-error">
            The script changed. Copy this draft or generate a new suggestion.
          </p>{/if}
        <div class="row">
          <Button
            variant="primary"
            disabled={invalid}
            onclick={() => onapply(current, !!current.range || current.operation === "new")}
            >{current.operation === "new"
              ? "Replace script"
              : current.range
                ? "Replace selection"
                : "Insert after"}<ChevronRight size={15} /></Button
          >
          {#if current.range && current.operation !== "new"}<Button
              variant="ghost"
              aria-label="Insert after selection"
              disabled={invalid}
              onclick={() => onapply(current, false)}><CornerDownRight size={16} /></Button
            >{/if}
          <Button
            variant="ghost"
            aria-label="Copy suggestion"
            onclick={() => navigator.clipboard.writeText(current.text)}><Copy size={16} /></Button
          >
          <Button
            variant="ghost"
            aria-label="Discard suggestion"
            onclick={() => ondiscard(current.id)}><Trash2 size={16} /></Button
          >
        </div>
      {:else}<p class="caption muted">Review first. Inserting is one undoable edit.</p>{/if}
      <Button variant="ghost" class="compact-compose" onclick={() => (view = "compose")}
        >Edit instruction</Button
      >
    </div>
  </section>
</div>
