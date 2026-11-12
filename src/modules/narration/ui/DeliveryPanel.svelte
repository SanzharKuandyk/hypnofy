<script lang="ts">
  import { Link2 } from "@lucide/svelte";
  import { Button, Field, Select, Slider, NumberField } from "../../../shared/ui";
  import type { SessionSettings, Voice } from "../../../shared/rpc/generated/bindings";
  import type { DeliveryOverride } from "../../../domain/document";

  let {
    session,
    voices,
    hasSelection,
    onscript,
    onapply,
    oncontinuous,
    onclose,
  }: {
    session: SessionSettings;
    voices: Voice[];
    hasSelection: boolean;
    onscript: (patch: Partial<SessionSettings>) => void;
    onapply: (value: DeliveryOverride) => void;
    oncontinuous: () => void;
    onclose: () => void;
  } = $props();

  let scope = $derived<"script" | "selection">(hasSelection ? "selection" : "script");
  let voice = $state("");
  let pace = $state<number | null>(null);
  const speed = $derived(scope === "script" ? (session.speed ?? 1) : (pace ?? session.speed ?? 1));

  function setPace(value: number): void {
    if (scope === "script") onscript({ speed: value });
    else pace = value;
  }
</script>

<div class="delivery-scopes" aria-label="Delivery scope">
  <Button aria-pressed={scope === "script"} onclick={() => (scope = "script")}>Whole script</Button>
  <Button aria-pressed={scope === "selection"} onclick={() => (scope = "selection")}
    >{hasSelection ? "Selected text" : "Current passage"}</Button
  >
</div>
<div class="delivery-body">
  <p class="muted">
    {scope === "script"
      ? "The starting voice and pace for this script."
      : "Change only this passage. Everything else keeps its defaults."}
  </p>
  <Field label="Voice"
    ><Select
      label={scope === "script" ? "Script voice" : "Passage voice"}
      value={scope === "script" ? (session.voiceId ?? "") : voice}
      onchange={(value) => {
        if (scope === "script") onscript({ voiceId: value || null });
        else voice = value;
      }}
      options={[
        { value: "", label: scope === "script" ? "Choose a voice" : "Inherit script voice" },
        ...voices.map((v) => ({ value: v.id, label: `${v.name} · ${v.language}` })),
      ]}
    /></Field
  >
  {#if !voices.length}<p class="caption muted">
      Install or link a speech model in Models & voices first.
    </p>{/if}
  <Field label="Pace"
    ><div class="pace-presets">
      {#each [{ label: "Slow", value: 0.8 }, { label: "Natural", value: 1 }, { label: "Brisk", value: 1.15 }] as preset (preset.value)}<Button
          aria-pressed={speed === preset.value}
          onclick={() => setPace(preset.value)}>{preset.label}</Button
        >{/each}
    </div></Field
  >
  <details class="disclosure">
    <summary>More timing options <span class="muted">{speed.toFixed(2)}×</span></summary>
    <Field label={`Fine pace · ${speed.toFixed(2)}×`}
      ><Slider
        label={scope === "script" ? "Script pace" : "Passage pace"}
        min={0.5}
        max={2}
        step={0.05}
        value={speed}
        onchange={setPace}
      /></Field
    >
    {#if scope === "script"}<Field label="Between paragraphs · seconds"
        ><NumberField
          label="Paragraph pause"
          min={0}
          max={30}
          step={0.5}
          value={session.paragraphPauseMs / 1000}
          onchange={(value) => onscript({ paragraphPauseMs: Math.round(value * 1000) })}
        /></Field
      >
    {:else}<Button variant="ghost" onclick={oncontinuous}
        ><Link2 size={15} />Make passages continuous</Button
      >
      <p class="caption muted">
        Removes extra paragraph gaps in this group. Natural speech pauses remain.
      </p>{/if}
  </details>
  <details class="disclosure">
    <summary>About this voice</summary>
    <p class="caption muted">
      Kokoro and Supertonic support voice choice and pace. They do not accept free-form acting or
      emotion instructions. Written pause cues become actual silence in the narration pipeline.
    </p>
  </details>
</div>
<div class="delivery-footer">
  {#if scope === "selection"}<Button
      variant="ghost"
      onclick={() => {
        onapply({ voiceId: null, speed: null });
        onclose();
      }}>Use script defaults</Button
    ><Button
      variant="primary"
      onclick={() => {
        onapply({ voiceId: voice || null, speed: pace });
        onclose();
      }}>Apply to passage</Button
    >
  {:else}<Button variant="primary" onclick={onclose}>Done</Button>{/if}
</div>
