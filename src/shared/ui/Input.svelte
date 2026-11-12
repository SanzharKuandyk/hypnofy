<script lang="ts">
  import { Eye, EyeOff } from "@lucide/svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLInputAttributes, "value" | "oninput" | "onchange" | "type"> {
    element?: HTMLInputElement | undefined;
    value?: string;
    type?: "text" | "password" | "search";
    oninput?: (value: string) => void;
    onchange?: (value: string) => void;
  }

  let {
    element = $bindable(),
    value = $bindable(""),
    type = "text",
    oninput,
    onchange,
    class: className = "",
    ...rest
  }: Props = $props();
  let reveal = $state(false);
</script>

{#if type === "password"}<div class="password-field">
    <input
      bind:this={element}
      {...rest}
      class={`control-input ${className}`}
      type={reveal ? "text" : "password"}
      {value}
      oninput={(event) => {
        value = event.currentTarget.value;
        oninput?.(value);
      }}
      onchange={() => onchange?.(value)}
    /><button
      type="button"
      class="password-toggle"
      aria-label={reveal ? "Hide API key" : "Show API key"}
      aria-pressed={reveal}
      onclick={() => (reveal = !reveal)}
      >{#if reveal}<EyeOff size={16} />{:else}<Eye size={16} />{/if}</button
    >
  </div>
{:else}<input
    bind:this={element}
    {...rest}
    class={`control-input ${className}`}
    {type}
    {value}
    oninput={(event) => {
      value = event.currentTarget.value;
      oninput?.(value);
    }}
    onchange={() => onchange?.(value)}
  />{/if}
