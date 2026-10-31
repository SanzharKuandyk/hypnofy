<script lang="ts">
  import { Checkbox as Primitive } from "bits-ui";
  import { Check, Minus } from "@lucide/svelte";

  let {
    checked = $bindable(false),
    indeterminate = false,
    disabled = false,
    label,
    onchange,
  }: {
    checked?: boolean;
    indeterminate?: boolean;
    disabled?: boolean;
    label: string;
    onchange?: (checked: boolean) => void;
  } = $props();
</script>

<Primitive.Root
  {checked}
  {indeterminate}
  {disabled}
  class="checkbox"
  aria-label={label}
  onCheckedChange={(value) => {
    checked = value;
    onchange?.(value);
  }}
>
  {#if indeterminate}<Minus size={14} aria-hidden="true" />{:else if checked}<Check
      size={14}
      aria-hidden="true"
    />{/if}
</Primitive.Root>
