<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, type ProjectInfo } from "./api";
  import { toast } from "./ui.svelte";

  let {
    folder = "",
    onClose,
    onCreated,
  }: { folder?: string; onClose: () => void; onCreated: (p: ProjectInfo) => void } = $props();

  type Kind = "article" | "blank" | "clipboard";
  let name = $state("");
  let kind = $state<Kind>("article");
  let clip = $state<{ ok: boolean; title: string | null }>({ ok: false, title: null });
  let busy = $state(false);

  onMount(async () => {
    try {
      const text = await api.clipboardText();
      const ok = text.includes("\\begin{document}");
      const m = /\\title\s*(?:\[[^\]]*\])?\s*\{((?:[^{}]|\{[^{}]*\})*)\}/.exec(text);
      const raw = m ? m[1].split("\\\\")[0] : "";
      const title = raw.replace(/\\[a-zA-Z]+\*?|[{}]/g, " ").replace(/\s+/g, " ").trim() || null;
      clip = { ok, title };
      if (ok) kind = "clipboard";
    } catch {
      /* no clipboard access */
    }
  });

  const options: { id: Kind; label: string }[] = [
    { id: "article", label: "Article" },
    { id: "blank", label: "Blank" },
    { id: "clipboard", label: "From Clipboard" },
  ];

  const hint = $derived(
    kind === "article"
      ? "11pt article with AMS math, graphicx and hyperref."
      : kind === "blank"
        ? "Just \\documentclass{article} and an empty document."
        : clip.title
          ? `Uses the LaTeX document on your clipboard: “${clip.title}”.`
          : "Uses the LaTeX document on your clipboard.",
  );

  async function create() {
    if (busy) return;
    busy = true;
    try {
      onCreated(await api.createProject(name.trim() || (kind === "clipboard" ? "" : "Untitled"), kind, folder));
    } catch (e) {
      toast(errorText(e), "error");
      busy = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      create();
    }
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<svelte:window onkeydown={key} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="scrim" onmousedown={onClose}></div>
<div class="dialog popover" role="dialog" aria-label="New paper">
  <h2>New Paper{#if folder}<span class="in"> in {folder.split("/").pop()}</span>{/if}</h2>
  <label class="field">
    <span>Name</span>
    <input
      bind:value={name}
      placeholder={kind === "clipboard" && clip.title ? clip.title : "Untitled"}
      use:autofocus
      spellcheck="false"
      autocomplete="off"
    />
  </label>
  <div class="field">
    <span>Template</span>
    <div class="seg" role="radiogroup">
      {#each options as o}
        <button
          role="radio"
          aria-checked={kind === o.id}
          class:on={kind === o.id}
          disabled={o.id === "clipboard" && !clip.ok}
          title={o.id === "clipboard" && !clip.ok ? "Copy a complete .tex document first" : ""}
          onclick={() => (kind = o.id)}>{o.label}</button
        >
      {/each}
    </div>
    <p class="hint">{hint}</p>
  </div>
  <div class="foot">
    <button class="btn secondary" onclick={onClose}>Cancel</button>
    <button class="btn primary" onclick={create} disabled={busy}>Create</button>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(0, 0, 0, 0.32);
  }
  .dialog {
    position: fixed;
    z-index: 61;
    top: 20%;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, calc(100vw - 32px));
    padding: 24px;
    animation: pop-in 0.18s ease-out;
  }
  h2 {
    margin: 0 0 20px;
    font-size: 15px;
    line-height: 20px;
    font-weight: 600;
  }
  .in {
    font-weight: 400;
    color: var(--text-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }
  .field > span {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  input {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    transition: border-color 0.12s ease-out, box-shadow 0.12s ease-out;
  }
  input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: 8px;
    background: var(--hover);
  }
  .seg button {
    flex: 1;
    height: 26px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .seg button:hover:not(:disabled):not(.on) {
    color: var(--text);
  }
  .seg button.on {
    background: var(--bg-raised);
    color: var(--text);
    box-shadow: 0 0 0 0.5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, 0.15);
  }
  .seg button:disabled {
    opacity: 0.4;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 24px;
  }
</style>
