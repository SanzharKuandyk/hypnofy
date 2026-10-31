<script lang="ts">
  import { Minus, Plus } from "@lucide/svelte";

  let {
    value = $bindable(0),
    min,
    max,
    step = 1,
    label,
    id,
    disabled = false,
    invalid = false,
    style = "",
    onchange,
    oncommit,
  }: {
    value?: number;
    min: number;
    max: number;
    step?: number;
    label: string;
    id?: string;
    disabled?: boolean;
    invalid?: boolean;
    style?: string;
    onchange?: (value: number) => void;
    oncommit?: (value: number) => void;
  } = $props();

  let draft = $state("");
  let focused = $state(false);
  let malformed = $state(false);
  $effect(() => {
    if (!focused) draft = String(value);
  });

  function update(next: number, commit: boolean): void {
    if (!Number.isFinite(next) || next < min || next > max) {
      malformed = true;
      return;
    }
    malformed = false;
    value = Number(next.toFixed(6));
    draft = String(value);
    onchange?.(value);
    if (commit) oncommit?.(value);
  }

  function adjust(direction: number): void {
    update(Math.min(max, Math.max(min, value + direction * step)), true);
  }
</script>

<div class="number-field" class:invalid={invalid || malformed} class:disabled {style}>
  <button
    type="button"
    class="stepper"
    aria-label={`Decrease ${label}`}
    disabled={disabled || value <= min}
    onclick={() => adjust(-1)}><Minus size={14} aria-hidden="true" /></button
  >
  <input
    {id}
    type="number"
    aria-label={label}
    aria-invalid={invalid || malformed}
    {min}
    {max}
    {step}
    {disabled}
    value={draft}
    onfocus={() => (focused = true)}
    oninput={(event) => {
      draft = event.currentTarget.value;
      update(event.currentTarget.valueAsNumber, false);
    }}
    onkeydown={(event) => {
      if (event.key === "Enter") event.currentTarget.blur();
    }}
    onblur={() => {
      focused = false;
      if (malformed) {
        malformed = false;
        draft = String(value);
      } else oncommit?.(value);
    }}
  />
  <button
    type="button"
    class="stepper"
    aria-label={`Increase ${label}`}
    disabled={disabled || value >= max}
    onclick={() => adjust(1)}><Plus size={14} aria-hidden="true" /></button
  >
</div>
