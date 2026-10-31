<script lang="ts">
  import {
    Button,
    Badge,
    Field,
    Dialog,
    Input,
    Progress,
    TooltipProvider,
    Select,
    Checkbox,
    Slider,
    NumberField,
    Textarea,
  } from "../ui";

  let open = $state(false);
  let text = $state("");
  let voice = $state("F4");
  let checked = $state(false);
  let speed = $state(1);
  let volume = $state(0.6);
  let mixedExport = $state(false);
  const voices = Array.from({ length: 40 }, (_, index) => ({
    value: `F${index + 1}`,
    label: `F${index + 1} · English`,
    disabled: index === 1,
  }));
</script>

<TooltipProvider>
  <div class="page-scroll" style="height:100%">
    <main class="page">
      <header>
        <h1>Hypnofy components</h1>
        <p class="muted">Development preview · /?showcase</p>
      </header>
      <section class="card">
        <h2>Designed controls</h2>
        <Field label="Voice"
          ><Select label="Preview voice" bind:value={voice} options={voices} /></Field
        >
        <Select label="Disabled voice" value="F4" options={voices} disabled />
        <Select
          label="Invalid voice"
          value=""
          options={voices}
          invalid
          placeholder="Choose a voice"
        />
        <Textarea
          aria-label="Preview instruction"
          placeholder="Make the language gentle, grounded and unhurried…"
        />
        <NumberField label="Preview pace" min={0.5} max={2} step={0.05} bind:value={speed} />
        <Slider label="Preview volume" bind:value={volume} />
        <div class="row">
          <Checkbox label="Preview selection" bind:checked /><Checkbox
            label="Mixed selection"
            indeterminate
          /><Checkbox label="Disabled selection" disabled />
        </div>
        <Progress label="Indeterminate progress" />
        <Select
          label="Preview export"
          bind:value={mixedExport}
          options={[
            { value: false, label: "Narration only" },
            { value: true, label: "Narration + ambience" },
          ]}
        />
        <Button aria-label="Preview tooltip">Hover or focus for help</Button>
      </section>
      <section class="card">
        <h2>Actions</h2>
        <div class="row">
          <Button variant="primary">Primary</Button><Button>Secondary</Button><Button
            variant="ghost">Quiet</Button
          ><Button variant="danger">Delete</Button><Button disabled>Unavailable</Button><Button
            loading>Working</Button
          >
        </div>
      </section>
      <section class="card">
        <h2>Fields & feedback</h2>
        <Field label="Title" help="A short hint that explains the field."
          ><Input
            aria-label="Preview title"
            bind:value={text}
            placeholder="A quiet evening"
          /></Field
        >
        <div class="row">
          <Badge tone="ready">Ready</Badge><Badge tone="warning">Needs verification</Badge><Badge
            tone="error">Unavailable</Badge
          >
        </div>
        <p class="inline-error">A recoverable error keeps the user’s work available.</p>
        <Progress class="progress" value={0.6} label="Operation progress" />
      </section>
      <section class="card">
        <h2>Dialogs</h2>
        <Button onclick={() => (open = true)}>Open dialog</Button>
      </section>
    </main>
  </div>
  <Dialog
    bind:open
    title="A focused conversation"
    description="Keyboard focus stays inside this dialog."
    ><Field label="Name"><Input aria-label="Preview collection name" /></Field>
    <Select label="Dialog voice" bind:value={voice} options={voices} />
    <div class="actions">
      <Button onclick={() => (open = false)}>Cancel</Button><Button
        variant="primary"
        onclick={() => (open = false)}>Continue</Button
      >
    </div></Dialog
  >
</TooltipProvider>
