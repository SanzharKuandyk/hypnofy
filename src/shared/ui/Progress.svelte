<script lang="ts">
  import { Progress as Primitive } from "bits-ui";

  let {
    value = null,
    label = "Progress",
    class: className = "",
  }: { value?: number | null; label?: string; class?: string } = $props();
  const fraction = $derived(value === null ? null : Math.min(1, Math.max(0, value)));
</script>

<Primitive.Root
  class={`control-progress ${className}`}
  value={fraction === null ? null : fraction * 100}
  aria-label={label}
>
  <span
    class="progress-fill"
    class:indeterminate={fraction === null}
    style:width={fraction === null ? "30%" : `${fraction * 100}%`}
  ></span>
</Primitive.Root>
