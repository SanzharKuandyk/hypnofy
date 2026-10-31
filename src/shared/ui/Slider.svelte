<script lang="ts">
  import { Slider as Primitive } from "bits-ui";

  let {
    value = $bindable(0),
    min = 0,
    max = 1,
    step = 0.01,
    disabled = false,
    label,
    class: className = "",
    style = "",
    onchange,
    oncommit,
  }: {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    label: string;
    class?: string;
    style?: string;
    onchange?: (value: number) => void;
    oncommit?: (value: number) => void;
  } = $props();
</script>

<Primitive.Root
  type="single"
  {value}
  {min}
  {max}
  {step}
  {disabled}
  class={`control-slider ${className}`}
  {style}
  onValueChange={(next) => {
    value = next;
    onchange?.(next);
  }}
  onValueCommit={(next) => oncommit?.(next)}
>
  <span class="slider-track"><Primitive.Range class="slider-range" /></span>
  <Primitive.Thumb index={0} class="slider-thumb" aria-label={label} />
</Primitive.Root>
