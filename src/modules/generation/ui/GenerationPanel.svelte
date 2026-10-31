<script lang="ts">
  import { Button, Field, Badge } from "../../../shared/ui";
  import { api } from "../../../shared/rpc/client";
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
  } = $props();
  let instruction = $state("");
  let operation = $state("continue");
  const running = $derived(job && ["queued", "running", "cancelling"].includes(job.state));
  const ready = $derived(
    models.filter((model) => model.capability === "text" && model.status === "ready"),
  );
</script>

<div class="form-grid">
  <div class="row spread">
    <h2>Write with AI</h2>
    <Button variant="ghost" aria-label="Close AI panel" onclick={onclose}>×</Button>
  </div>
  <Field label="Writing model"
    ><select aria-label="Writing model" bind:value={modelId}
      ><option value="">Choose a model</option>{#each ready as model (model.id)}<option
          value={model.id}
          >{model.name} · {model.provider === "openrouter" ? "Cloud" : "Local"}</option
        >{/each}</select
    ></Field
  >
  {#if models.find((m) => m.id === modelId)?.provider === "openrouter"}<p class="caption muted">
      This request sends the current script context to OpenRouter.
    </p>{:else}<Badge tone="ready">Runs on your device</Badge>{/if}
  <Field label="Operation"
    ><select aria-label="Draft operation" bind:value={operation}
      ><option value="continue">Continue script</option><option value="new">New draft</option
      ><option value="rewrite-selection" disabled={!selectedText}>Rewrite selection</option></select
    ></Field
  >
  <Field label="Instruction"
    ><textarea
      aria-label="AI instruction"
      bind:value={instruction}
      placeholder="Make the language gentle, grounded and unhurried…"></textarea></Field
  >
  <p class="caption muted">
    Context: current saved script{selectedText ? " and selected passage" : ""}.
  </p>
  {#if running}<Button onclick={() => job && api.jobs.cancel(job.id)}>Cancel generation</Button>
  {:else}<Button
      variant="primary"
      disabled={!ready.some((model) => model.id === modelId) || !instruction.trim()}
      onclick={() => onstart(instruction, operation)}>Generate suggestion</Button
    >{/if}
  {#if job?.error}<p class="inline-error">{job.error.message}</p>{/if}
  {#if running && job?.text}<div class="suggestion" aria-live="polite">{job.text}</div>{/if}
  {#each suggestions as suggestion (suggestion.id)}
    {@const invalid =
      stale ||
      suggestion.baseRevision !== baseRevision ||
      suggestion.baseContentHash !== baseContentHash}
    <div class="card">
      <h3>Suggested passage</h3>
      {#if invalid}<p class="inline-error">
          The script changed since this suggestion was generated. Copy it or generate a new
          suggestion.
        </p>{/if}
      <div class="suggestion">{suggestion.text}</div>
      <Button
        variant="primary"
        disabled={invalid}
        onclick={() => onapply(suggestion, !!suggestion.range || suggestion.operation === "new")}
        >{suggestion.range
          ? "Replace selection"
          : suggestion.operation === "new"
            ? "Replace script"
            : "Insert after"}</Button
      >
      {#if selectedText}<Button disabled={stale} onclick={() => onapply(suggestion, false)}
          >Insert after selection</Button
        >{/if}
      <div class="row">
        <Button variant="ghost" onclick={() => navigator.clipboard.writeText(suggestion.text)}
          >Copy</Button
        ><Button variant="ghost" onclick={() => ondiscard(suggestion.id)}>Discard</Button>
      </div>
    </div>
  {/each}
</div>
