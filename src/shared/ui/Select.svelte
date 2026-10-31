<script lang="ts" generics="T extends SelectValue">
  import { Select as Primitive } from "bits-ui";
  import { Check, ChevronDown } from "@lucide/svelte";
  import type { SelectOption, SelectValue } from "./control";

  let {
    value = $bindable(),
    options,
    label,
    placeholder = "Choose…",
    disabled = false,
    invalid = false,
    class: className = "",
    style = "",
    onchange,
    onclose,
  }: {
    value: T;
    options: readonly SelectOption<T>[];
    label: string;
    placeholder?: string;
    disabled?: boolean;
    invalid?: boolean;
    class?: string;
    style?: string;
    onchange?: (value: T) => void | Promise<void>;
    onclose?: () => void;
  } = $props();

  const selection = $derived(options.find((option) => Object.is(option.value, value)));
  let trigger = $state<HTMLButtonElement | null>(null);
  const items = $derived(
    options.map((option) => ({
      value: JSON.stringify(option.value),
      label: option.label,
      disabled: option.disabled ?? false,
    })),
  );

  function change(encoded: string): void {
    const option = options.find((option) => JSON.stringify(option.value) === encoded);
    if (!option || option.disabled) return;
    value = option.value;
    void onchange?.(value);
  }
</script>

<Primitive.Root
  type="single"
  value={selection ? JSON.stringify(selection.value) : ""}
  {items}
  {disabled}
  allowDeselect={false}
  loop
  onValueChange={change}
  onOpenChangeComplete={(open) => {
    if (!open) onclose?.();
  }}
>
  <Primitive.Trigger
    bind:ref={trigger}
    class={`ui-select ${className}`}
    {style}
    aria-label={label}
    aria-invalid={invalid}
  >
    <span class="ui-select-value" class:muted={!selection}>{selection?.label ?? placeholder}</span
    ><ChevronDown size={16} aria-hidden="true" />
  </Primitive.Trigger>
  <Primitive.Portal>
    <Primitive.Content class="control-menu" sideOffset={6} collisionPadding={12}>
      <Primitive.Viewport
        class="control-options"
        onscrollend={() => trigger?.focus({ preventScroll: true })}
      >
        {#each options as option (JSON.stringify(option.value))}
          <Primitive.Item
            class="control-option"
            value={JSON.stringify(option.value)}
            label={option.label}
            disabled={option.disabled ?? false}
            aria-disabled={option.disabled ?? false}
          >
            <span class="control-option-copy"
              ><span>{option.label}</span>{#if option.description}<small>{option.description}</small
                >{/if}</span
            >
            <span class="option-check" aria-hidden="true"
              >{#if Object.is(option.value, value)}<Check size={16} />{/if}</span
            >
          </Primitive.Item>
        {:else}<div class="control-menu-empty">No options available</div>{/each}
      </Primitive.Viewport>
    </Primitive.Content>
  </Primitive.Portal>
</Primitive.Root>
