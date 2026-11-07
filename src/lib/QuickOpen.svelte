<script lang="ts">
  let { files, onPick, onClose }: { files: string[]; onPick: (rel: string) => void; onClose: () => void } = $props();

  let q = $state("");
  let sel = $state(0);

  function score(path: string, query: string): number {
    if (!query) return 1;
    const p = path.toLowerCase();
    const s = query.toLowerCase();
    const name = p.split("/").pop()!;
    if (name.startsWith(s)) return 1000 - name.length;
    if (name.includes(s)) return 800 - name.length;
    // Subsequence match.
    let i = 0;
    let gaps = 0;
    for (const ch of p) {
      if (ch === s[i]) i++;
      else if (i > 0) gaps++;
      if (i === s.length) return 400 - gaps;
    }
    return -1;
  }

  const results = $derived(
    files
      .map((f) => ({ f, s: score(f, q) }))
      .filter((r) => r.s >= 0)
      .sort((a, b) => b.s - a.s || a.f.localeCompare(b.f))
      .slice(0, 50)
      .map((r) => r.f),
  );

  $effect(() => {
    void q;
    sel = 0;
  });

  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(results.length - 1, sel + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (results[sel]) onPick(results[sel]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="scrim" onmousedown={onClose}></div>
<div class="palette popover">
  <input placeholder="Go to file" bind:value={q} onkeydown={key} use:autofocus spellcheck="false" autocomplete="off" />
  <div class="list">
    {#each results as f, i (f)}
      {@const parts = f.split("/")}
      <button class:sel={i === sel} onmouseenter={() => (sel = i)} onclick={() => onPick(f)}>
        <span class="name">{parts[parts.length - 1]}</span>
        {#if parts.length > 1}<span class="dir">{parts.slice(0, -1).join("/")}</span>{/if}
      </button>
    {:else}
      <div class="none">No matching files</div>
    {/each}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 80;
  }
  .palette {
    position: fixed;
    z-index: 81;
    top: 80px;
    left: 50%;
    transform: translateX(-50%);
    width: min(520px, calc(100vw - 32px));
    overflow: hidden;
    animation: pop-in 0.12s ease-out;
  }
  input {
    width: 100%;
    height: 44px;
    padding: 0 16px;
    border: none;
    border-bottom: 1px solid var(--line);
    background: transparent;
    color: var(--text);
    font: 400 15px var(--font-ui);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .list {
    max-height: 336px;
    overflow: auto;
    padding: 4px;
  }
  button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    text-align: left;
  }
  button.sel {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .dir {
    font-size: 12px;
    opacity: 0.6;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    padding: 12px;
    color: var(--text-3);
  }
</style>
