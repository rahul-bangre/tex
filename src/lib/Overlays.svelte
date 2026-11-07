<script lang="ts">
  import { ui, closeMenu } from "./ui.svelte";

  let menuEl = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    const m = ui.menu;
    if (!m || !menuEl) return;
    const r = menuEl.getBoundingClientRect();
    pos = {
      x: Math.min(m.x, window.innerWidth - r.width - 8),
      y: m.y + r.height > window.innerHeight - 8 ? Math.max(8, m.y - r.height) : m.y,
    };
    menuEl.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (!ui.menu) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const btns = [...(menuEl?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      const i = btns.indexOf(document.activeElement as HTMLButtonElement);
      const n = e.key === "ArrowDown" ? (i + 1) % btns.length : (i - 1 + btns.length) % btns.length;
      btns[n]?.focus();
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} />

{#if ui.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="scrim" onmousedown={closeMenu} oncontextmenu={(e) => { e.preventDefault(); closeMenu(); }}></div>
  <div class="menu popover" role="menu" bind:this={menuEl} style="left:{pos.x || ui.menu.x}px;top:{pos.y || ui.menu.y}px">
    {#each ui.menu.items as item}
      {#if item === "sep"}
        <div class="sep"></div>
      {:else}
        <button
          role="menuitem"
          class:danger={item.danger}
          disabled={item.disabled}
          onclick={() => {
            closeMenu();
            item.action();
          }}
        >
          <span>{item.label}</span>
          {#if item.hint}<kbd>{item.hint}</kbd>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<div class="toasts" aria-live="polite">
  {#each ui.toasts as t (t.id)}
    <div class="toast" class:error={t.tone === "error"}>{t.text}</div>
  {/each}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .menu {
    position: fixed;
    z-index: 91;
    min-width: 200px;
    padding: 4px;
    animation: pop-in 0.12s ease-out;
  }
  .menu button {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    height: 24px;
    padding: 0 8px;
    border-radius: 5px;
    text-align: left;
    white-space: pre;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--accent);
    color: var(--accent-ink);
    outline: none;
  }
  .menu button:hover kbd,
  .menu button:focus-visible kbd {
    color: inherit;
    opacity: 0.75;
  }
  .menu button.danger {
    color: var(--err);
  }
  .menu button.danger:hover,
  .menu button.danger:focus-visible {
    background: var(--err);
    color: #fff;
  }
  .menu button:disabled {
    color: var(--text-3);
  }
  .sep {
    height: 1px;
    background: var(--line);
    margin: 4px 8px;
  }
  .toasts {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    z-index: 100;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    max-width: 520px;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 12px;
    background: var(--bg-raised);
    color: var(--text);
    box-shadow: var(--shadow-pop);
    animation: pop-in 0.18s ease-out;
  }
  .toast.error {
    background: var(--err);
    color: #fff;
  }
</style>
