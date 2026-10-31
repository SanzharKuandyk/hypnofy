<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLTextareaAttributes, "value" | "oninput"> {
    value?: string;
    oninput?: (value: string) => void;
  }

  let { value = $bindable(""), oninput, class: className = "", ...rest }: Props = $props();
</script>

<textarea
  {...rest}
  class={`control-textarea ${className}`}
  {value}
  oninput={(event) => {
    value = event.currentTarget.value;
    oninput?.(value);
  }}></textarea>
