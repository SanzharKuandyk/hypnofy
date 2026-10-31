<script lang="ts">
  import { Play, Pause, Square, Volume2, Clock, Download } from "@lucide/svelte";
  import { Button, Select, Slider } from "../../../shared/ui";
  import type { PlaybackState, RenderRecord } from "../../../domain";

  let {
    state,
    render,
    onplay,
    oncontrol,
    onexport,
  }: {
    state: PlaybackState | null;
    render: Pick<RenderRecord, "id" | "durationMs"> | undefined;
    onplay: () => void;
    oncontrol: (operation: string, value?: number) => void;
    onexport: () => void;
  } = $props();

  const loaded = $derived(!!render && state?.renderId === render.id);

  function time(ms: number): string {
    return `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
  }
</script>

<div class="player" aria-label="Audio player">
  <div class="player-main">
    <Button
      variant="primary"
      class="icon"
      aria-label={loaded && state?.playing ? "Pause playback" : "Play narration"}
      disabled={!render}
      onclick={onplay}
      >{#if loaded && state?.playing}<Pause size={18} />{:else}<Play size={18} />{/if}</Button
    ><Button
      variant="ghost"
      class="icon"
      aria-label="Stop playback"
      disabled={!loaded}
      onclick={() => oncontrol("stop")}><Square size={16} /></Button
    ><span class="caption"
      >{time(loaded ? (state?.positionMs ?? 0) : 0)} / {time(render?.durationMs ?? 0)}</span
    ><span class="render"
      ><Button disabled={!render} onclick={onexport}><Download size={15} />Export WAV</Button></span
    >
  </div>
  <Slider
    label="Playback position"
    min={0}
    max={Math.max(1, render?.durationMs ?? 0)}
    step={1}
    value={loaded ? (state?.positionMs ?? 0) : 0}
    disabled={!loaded}
    oncommit={(value) => oncontrol("seek", Number(value))}
  />
  <div class="player-detail">
    <Volume2 size={15} /><Slider
      label="Narration volume"
      min={0}
      max={1}
      step={0.01}
      value={state?.volume ?? 1}
      style="width:90px"
      oncommit={(value) => oncontrol("volume", Number(value))}
    /><span class="extra">Narration</span><Clock size={14} /><Select
      label="Sleep timer"
      style="padding:2px 6px"
      onchange={(value) => oncontrol("sleep", Number(value))}
      value="0"
      options={[
        { value: "0", label: `Sleep timer off` },
        { value: "10", label: `10 minutes` },
        { value: "20", label: `20 minutes` },
        { value: "30", label: `30 minutes` },
        { value: "60", label: `60 minutes` },
      ]}
    />{#if state?.sleepRemainingMs}<span>{time(state.sleepRemainingMs)} left</span
      >{/if}{#if state?.error}<span role="alert">{state.error}</span>{/if}
  </div>
</div>
