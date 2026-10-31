<script lang="ts">
  import { Moon } from "@lucide/svelte";
  import { Button, Dialog, Checkbox } from "../../../shared/ui";
  let {
    open = $bindable(true),
    help = false,
    onaccept,
    onexit,
  }: {
    open?: boolean;
    help?: boolean;
    onaccept: () => Promise<void>;
    onexit: () => void;
  } = $props();
  let checked = $state(false);
  let busy = $state(false);
  let error = $state("");
</script>

<Dialog
  bind:open
  title="Before you begin"
  description="A few important things about using Hypnofy."
  required={!help}
>
  <div class="notice-brand"><Moon size={22} /><span>Hypnofy</span></div>
  <div class="notice-copy">
    <section>
      <h3>Your health comes first</h3>
      <p>
        Hypnofy is a self-guided relaxation and script-writing tool. It does not diagnose or treat
        health conditions. If you have serious or persistent physical or mental health concerns,
        seek a qualified healthcare professional. Do not delay or replace professional care with
        this app.
      </p>
    </section>
    <section>
      <h3>Use safely</h3>
      <p>
        Listen only in a safe place, never while driving or using machinery. Stop if a session feels
        distressing. AI-generated scripts can be wrong; review them before listening.
      </p>
    </section>
    <section>
      <h3>Your choice and responsibility</h3>
      <p>
        Use is voluntary and at your own risk. No results are promised. To the extent permitted by
        applicable law, the author and contributors accept no liability for harm arising from use.
        This does not exclude liability that cannot legally be excluded.
      </p>
    </section>
    <a
      href="https://www.nhs.uk/tests-and-treatments/herbal-medicines-and-complementary-therapies/"
      target="_blank"
      rel="noreferrer">Read safety guidance ↗</a
    >
  </div>
  {#if !help}
    <label class="acknowledgment">
      <Checkbox bind:checked label="I have read the welcome notice" />
      <span
        >I have read this notice and understand that Hypnofy does not replace professional care.</span
      >
    </label>
  {/if}
  {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
  <div class="actions">
    {#if help}<Button onclick={() => (open = false)}>Done</Button>
    {:else}
      <Button onclick={onexit}>Exit app</Button>
      <Button
        variant="primary"
        disabled={!checked}
        loading={busy}
        onclick={async () => {
          busy = true;
          error = "";
          try {
            await onaccept();
            open = false;
          } catch (value) {
            error = value instanceof Error ? value.message : "Could not accept notice";
          } finally {
            busy = false;
          }
        }}>Accept & continue</Button
      >
    {/if}
  </div>
</Dialog>
