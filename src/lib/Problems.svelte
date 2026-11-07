<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import type { Diagnostic } from "./api";

  let {
    diagnostics,
    output,
    onGo,
    onClose,
  }: {
    diagnostics: Diagnostic[];
    output: string;
    onGo: (d: Diagnostic) => void;
    onClose: () => void;
  } = $props();

  let tab = $state<"problems" | "output">("problems");
  let showBoxes = $state(false);

  const main = $derived(diagnostics.filter((d) => d.severity !== "box"));
  const boxes = $derived(diagnostics.filter((d) => d.severity === "box"));
  const loc = (d: Diagnostic) => (d.file ? `${d.file}${d.line ? ":" + d.line : ""}` : "");
</script>

<section class="panel">
  <div class="pane-head">
    <div class="seg">
      <button class:on={tab === "problems"} onclick={() => (tab = "problems")}>
        Problems{#if main.length}<span class="count tnum">{main.length}</span>{/if}
      </button>
      <button class:on={tab === "output"} onclick={() => (tab = "output")}>Log</button>
    </div>
    <span class="spacer"></span>
    <button class="icon-btn" title="Close" onclick={onClose}><X size={16} strokeWidth={1.75} /></button>
  </div>

  {#if tab === "problems"}
    <div class="list">
      {#each main as d}
        <button class="item {d.severity}" onclick={() => onGo(d)} disabled={!d.file}>
          <span class="ic">
            {#if d.severity === "error"}<CircleAlert size={14} strokeWidth={2} />{:else}<TriangleAlert size={14} strokeWidth={2} />{/if}
          </span>
          <span class="msg">{d.message}</span>
          <span class="loc mono">{loc(d)}</span>
        </button>
      {/each}
      {#if boxes.length}
        <button class="item more" onclick={() => (showBoxes = !showBoxes)}>
          <span class="ic"></span>
          <span class="msg">{showBoxes ? "Hide" : "Show"} {boxes.length} overfull/underfull {boxes.length === 1 ? "box" : "boxes"}</span>
        </button>
        {#if showBoxes}
          {#each boxes as d}
            <button class="item box" onclick={() => onGo(d)} disabled={!d.file}>
              <span class="ic"></span>
              <span class="msg">{d.message}</span>
              <span class="loc mono">{loc(d)}</span>
            </button>
          {/each}
        {/if}
      {/if}
      {#if !main.length && !boxes.length}
        <div class="clean">No problems in the last build.</div>
      {/if}
    </div>
  {:else}
    <pre class="output mono">{output || "No log yet."}</pre>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
    border-top: 1px solid var(--line);
    min-height: 0;
  }
  .panel .pane-head {
    padding-left: 8px;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 7px;
    background: var(--hover);
  }
  .seg button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--bg-raised);
    color: var(--text);
    box-shadow: 0 0 0 0.5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, 0.12);
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 4px 8px 12px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 4px 8px;
    border-radius: 6px;
    text-align: left;
  }
  .item:hover:not(:disabled) {
    background: var(--hover);
  }
  .ic {
    width: 16px;
    flex: none;
    display: grid;
    place-items: center;
  }
  .error .ic {
    color: var(--err);
  }
  .warning .ic {
    color: var(--warn);
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    line-height: 18px;
  }
  .box .msg,
  .more .msg {
    color: var(--text-2);
  }
  .loc {
    flex: none;
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .clean {
    padding: 8px;
    color: var(--text-2);
  }
  .output {
    flex: 1;
    overflow: auto;
    margin: 0;
    padding: 8px 16px 16px;
    font-size: 12px;
    line-height: 18px;
    color: var(--text-2);
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
