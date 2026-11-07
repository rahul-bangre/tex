<script lang="ts">
  import { openMenu, prefs, type MenuEntry } from "./ui.svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Folder from "@lucide/svelte/icons/folder";
  import FileText from "@lucide/svelte/icons/file-text";
  import FileImage from "@lucide/svelte/icons/file-image";
  import FileCode from "@lucide/svelte/icons/file-code-2";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import File from "@lucide/svelte/icons/file";
  import type { Entry } from "./api";

  type Edit = { mode: "rename" | "file" | "folder"; target: string; value: string };

  let {
    entries,
    current,
    main,
    dirty,
    projectKey,
    onOpen,
    onCommit,
    onDelete,
    onSetMain,
    onReveal,
    edit = $bindable(null),
  }: {
    entries: Entry[];
    current: string | null;
    main: string | null;
    dirty: Set<string>;
    projectKey: string;
    onOpen: (rel: string) => void;
    onCommit: (e: Edit) => void;
    onDelete: (rel: string, dir: boolean) => void;
    onSetMain: (rel: string) => void;
    onReveal: (rel: string) => void;
    edit?: Edit | null;
  } = $props();

  interface Node {
    name: string;
    path: string;
    dir: boolean;
    children: Node[];
  }

  const tree = $derived.by(() => {
    const root: Node = { name: "", path: "", dir: true, children: [] };
    const byPath = new Map<string, Node>([["", root]]);
    const sorted = [...entries].sort((a, b) => a.path.localeCompare(b.path));
    for (const e of sorted) {
      const parts = e.path.split("/");
      const name = parts.pop()!;
      const parentPath = parts.join("/");
      let parent = byPath.get(parentPath);
      if (!parent) continue;
      const node: Node = { name, path: e.path, dir: e.dir, children: [] };
      parent.children.push(node);
      if (e.dir) byPath.set(e.path, node);
    }
    const sortRec = (n: Node) => {
      n.children.sort((a, b) => (a.dir !== b.dir ? (a.dir ? -1 : 1) : a.name.localeCompare(b.name, undefined, { numeric: true })));
      n.children.forEach(sortRec);
    };
    sortRec(root);
    return root;
  });

  // svelte-ignore state_referenced_locally
  let collapsed = $state<Set<string>>(new Set(prefs.get<string[]>("collapsed:" + projectKey, [])));

  function toggle(path: string) {
    const next = new Set(collapsed);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    collapsed = next;
    prefs.set("collapsed:" + projectKey, [...next]);
  }

  export function expand(path: string) {
    if (!collapsed.has(path)) return;
    const next = new Set(collapsed);
    next.delete(path);
    collapsed = next;
  }

  function kind(name: string): string {
    if (/\.(png|jpe?g|gif|svg|webp|eps|bmp|tiff?|pdf)$/i.test(name)) return "image";
    if (/\.bib$/i.test(name)) return "bib";
    if (/\.(tex|ltx)$/i.test(name)) return "tex";
    if (/\.(sty|cls|py|js|ts|json|sh|lua|r|m|html|toml|ya?ml)$/i.test(name)) return "code";
    return "file";
  }

  function ext(name: string): string {
    const m = /\.([a-z0-9]+)$/i.exec(name);
    return m ? m[1].toLowerCase() : "";
  }

  function menu(e: MouseEvent, n: Node) {
    const items: MenuEntry[] = [];
    if (n.dir) {
      items.push({ label: "New File", action: () => (edit = { mode: "file", target: n.path, value: "" }) });
      items.push({ label: "New Folder", action: () => (edit = { mode: "folder", target: n.path, value: "" }) });
      items.push("sep");
    } else if (n.name.endsWith(".tex") && n.path !== main) {
      items.push({ label: "Set as Main File", action: () => onSetMain(n.path) });
      items.push("sep");
    }
    items.push({ label: "Rename", action: () => (edit = { mode: "rename", target: n.path, value: n.name }) });
    items.push({ label: "Reveal in Finder", action: () => onReveal(n.path) });
    items.push("sep");
    items.push({ label: "Move to Trash", danger: true, action: () => onDelete(n.path, n.dir) });
    openMenu(e, items);
  }

  function focusSelect(el: HTMLInputElement) {
    el.focus();
    const dot = el.value.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : el.value.length);
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (edit && edit.value.trim()) onCommit({ ...edit, value: edit.value.trim() });
      edit = null;
    } else if (e.key === "Escape") {
      e.preventDefault();
      edit = null;
    }
  }
</script>

{#snippet icon(name: string, dir: boolean)}
  {@const k = dir ? "dir" : kind(name)}
  {#if k === "dir"}<Folder size={16} strokeWidth={1.75} />
  {:else if k === "image"}<FileImage size={16} strokeWidth={1.75} />
  {:else if k === "bib"}<BookOpen size={16} strokeWidth={1.75} />
  {:else if k === "tex"}<FileText size={16} strokeWidth={1.75} />
  {:else if k === "code"}<FileCode size={16} strokeWidth={1.75} />
  {:else}<File size={16} strokeWidth={1.75} />{/if}
{/snippet}

{#snippet editor(depth: number, name: string, dir: boolean)}
  <div class="row editing" style="--depth:{depth}">
    <span class="chev"></span>
    <span class="ic">{@render icon(name, dir)}</span>
    <input
      bind:value={edit!.value}
      use:focusSelect
      onkeydown={editKey}
      onblur={() => (edit = null)}
      placeholder={edit!.mode === "folder" ? "Folder name" : edit!.mode === "file" ? "name.tex" : ""}
      spellcheck="false"
      autocomplete="off"
    />
  </div>
{/snippet}

{#snippet nodes(list: Node[], depth: number, parent: string)}
  {#if edit && (edit.mode === "file" || edit.mode === "folder") && edit.target === parent}
    {@render editor(depth, edit.value || "x.tex", edit.mode === "folder")}
  {/if}
  {#each list as n (n.path)}
    {#if edit && edit.mode === "rename" && edit.target === n.path}
      {@render editor(depth, n.name, n.dir)}
    {:else if n.dir}
      <button class="row" style="--depth:{depth}" onclick={() => toggle(n.path)} oncontextmenu={(e) => menu(e, n)}>
        <span class="chev" class:open={!collapsed.has(n.path)}><ChevronRight size={14} strokeWidth={2} /></span>
        <span class="ic">{@render icon(n.name, true)}</span>
        <span class="nm">{n.name}</span>
      </button>
    {:else}
      <button
        class="row"
        class:active={current === n.path}
        style="--depth:{depth}"
        onclick={() => onOpen(n.path)}
        oncontextmenu={(e) => menu(e, n)}
        title={n.path === main ? n.path + " — main file" : n.path}
      >
        <span class="chev"></span>
        <span class="ic" class:main={n.path === main}>{@render icon(n.name, false)}</span>
        <span class="nm">{n.name}</span>
        {#if dirty.has(n.path)}<span class="dirty"></span>{/if}
        {#if n.path === main}<span class="badge">Main</span>{/if}
      </button>
    {/if}
    {#if n.dir && (!collapsed.has(n.path) || (edit && edit.target === n.path && edit.mode !== "rename"))}
      {@render nodes(n.children, depth + 1, n.path)}
    {/if}
  {/each}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tree"
  oncontextmenu={(e) => {
    if (e.target === e.currentTarget)
      openMenu(e, [
        { label: "New File", action: () => (edit = { mode: "file", target: "", value: "" }) },
        { label: "New Folder", action: () => (edit = { mode: "folder", target: "", value: "" }) },
      ]);
  }}
>
  {@render nodes(tree.children, 0, "")}
</div>

<style>
  .tree {
    padding: 4px 8px 24px;
    min-height: 100%;
  }
  .row {
    display: flex;
    align-items: center;
    width: 100%;
    height: 28px;
    padding: 0 8px 0 calc(var(--depth) * 16px);
    border-radius: 6px;
    text-align: left;
    color: var(--text);
  }
  .row:hover {
    background: var(--hover);
  }
  .row.active {
    background: var(--selected);
  }
  .chev {
    width: 20px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
    transition: transform 0.12s ease-out;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .ic {
    width: 16px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
    margin-right: 8px;
  }
  .ic.main {
    color: var(--accent);
  }
  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dirty {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
    margin-left: 8px;
    flex: none;
  }
  .badge {
    flex: none;
    margin-left: 8px;
    font-size: 11px;
    line-height: 16px;
    padding: 0 6px;
    border-radius: 4px;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .editing input {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 6px;
    font: inherit;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
  }
</style>
