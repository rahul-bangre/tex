<script lang="ts" module>
  // Survives going into a paper and back.
  const thumbUrls = new Map<string, { mtime: number; url: string }>();
  // Papers we've already tried to build in the background this session.
  const attempted = new Set<string>();
</script>

<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog, ask } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type ProjectInfo } from "./api";
  import { renderThumb } from "./pdf";
  import NewPaper from "./NewPaper.svelte";
  import { openMenu, prefs, relTime, toast, mod, type MenuEntry } from "./ui.svelte";
  import Plus from "@lucide/svelte/icons/plus";
  import Upload from "@lucide/svelte/icons/upload";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Search from "@lucide/svelte/icons/search";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Papers from "@lucide/svelte/icons/library-big";

  let { onOpen }: { onOpen: (path: string) => void } = $props();

  let projects = $state<ProjectInfo[]>([]);
  let folders = $state<string[]>([]);
  // null = All Papers; "" never used; otherwise a folder path relative to the library.
  let view = $state<string | null>(prefs.get<string | null>("lib.view", null));
  let folderEdit = $state<{ mode: "new" | "rename"; target: string; value: string } | null>(null);
  let drag = $state<{ path: string; name: string; x: number; y: number; over: string | null } | null>(null);
  let suppressClick = false;
  let loaded = $state(false);
  let root = $state("");
  let query = $state("");
  let sort = $state<"recent" | "name">(prefs.get("lib.sort", "recent"));
  let sel = $state(-1);
  let creating = $state(false);
  let dropping = $state(false);
  let building = $state<string | null>(null);
  let thumbs = $state<Record<string, string>>({});
  let pages = $state<Record<string, number>>(prefs.get("lib.pages", {}));
  let searchEl = $state<HTMLInputElement>();
  let gridEl = $state<HTMLElement>();
  let unlisten: UnlistenFn | null = null;
  let alive = true;

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const scoped = view === null ? projects : projects.filter((p) => !p.external && p.folder === view);
    const list = q
      ? scoped.filter((p) => p.name.toLowerCase().includes(q) || (p.title ?? "").toLowerCase().includes(q))
      : [...scoped];
    if (sort === "name") list.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    else list.sort((a, b) => Math.max(b.opened, b.modified) - Math.max(a.opened, a.modified));
    return list;
  });

  async function refresh() {
    try {
      const [info, lib] = await Promise.all([api.appInfo(), api.listLibrary()]);
      root = info.root;
      projects = lib.projects;
      folders = lib.folders;
      if (view !== null && !folders.includes(view)) setView(null);
      loaded = true;
      loadThumbs(lib.projects);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  let thumbRun = 0;
  async function loadThumbs(list: ProjectInfo[]) {
    const run = ++thumbRun;
    const next: Record<string, string> = {};
    for (const p of list) {
      const cached = thumbUrls.get(p.path);
      if (cached && cached.mtime === p.thumbMtime) next[p.path] = cached.url;
    }
    thumbs = next;
    const live = () => alive && run === thumbRun;

    // 1. Thumbnails already on disk.
    for (const p of list) {
      if (!live()) return;
      if (!p.thumb || next[p.path] || p.pdfMtime > p.thumbMtime) continue;
      try {
        setThumb(p.path, p.thumbMtime, new Blob([await api.readThumb(p.path)], { type: "image/png" }));
      } catch {
        /* missing */
      }
    }
    // 2. PDFs newer than their thumbnail.
    for (const p of list) {
      if (!live()) return;
      if (p.pdf && (p.pdfMtime > p.thumbMtime || pages[p.path] == null)) await makeThumb(p.path, p.pdf);
    }
    // 3. Papers never built: build them quietly, one at a time, so every card shows its real first page.
    for (const p of list) {
      if (!live()) return;
      if (p.pdf || !p.main || attempted.has(p.path)) continue;
      attempted.add(p.path);
      building = p.path;
      try {
        const r = await api.compile(p.path, p.main);
        if (r.pdf && live()) await makeThumb(p.path, r.pdf);
      } catch {
        /* tectonic missing or broken paper: leave the cover */
      } finally {
        if (building === p.path) building = null;
      }
    }
  }

  async function makeThumb(path: string, pdf: string) {
    try {
      const { png, pages: n } = await Promise.race([
        renderThumb(await api.readPdf(pdf)),
        new Promise<never>((_, rej) => setTimeout(() => rej(new Error("thumbnail timed out")), 20000)),
      ]);
      await api.saveThumb(path, png);
      setThumb(path, Date.now(), new Blob([png as BlobPart], { type: "image/png" }));
      pages = { ...pages, [path]: n };
      prefs.set("lib.pages", pages);
    } catch {
      /* unreadable PDF */
    }
  }

  function setThumb(path: string, mtime: number, blob: Blob) {
    const old = thumbUrls.get(path);
    if (old) URL.revokeObjectURL(old.url);
    const url = URL.createObjectURL(blob);
    thumbUrls.set(path, { mtime, url });
    thumbs = { ...thumbs, [path]: url };
  }

  async function openFolder() {
    const dir = await openDialog({ directory: true, title: "Open a folder that contains a LaTeX project" });
    if (typeof dir !== "string") return;
    try {
      onOpen((await api.addFolder(dir)).path);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function importZip(path?: string) {
    let file = path;
    if (!file) {
      const picked = await openDialog({
        title: "Import an Overleaf project",
        filters: [{ name: "Zip archive", extensions: ["zip"] }],
      });
      if (typeof picked !== "string") return;
      file = picked;
    }
    try {
      const p = await api.importZip(file, view ?? "");
      toast(`Imported “${p.name}”`);
      onOpen(p.path);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function changeRoot() {
    const dir = await openDialog({ directory: true, title: "Choose where tex keeps your papers" });
    if (typeof dir !== "string") return;
    await api.setRoot(dir);
    for (const v of thumbUrls.values()) URL.revokeObjectURL(v.url);
    thumbUrls.clear();
    refresh();
  }

  function libraryMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ clientX: r.right - 220, clientY: r.bottom + 4 }, [
      { label: "Show Library in Finder", action: () => api.reveal(root) },
      { label: "Change Library Folder…", action: changeRoot },
      "sep",
      { label: `Library: ${root.replace(/^\/Users\/[^/]+/, "~")}`, disabled: true, action: () => {} },
    ]);
  }

  function cardMenu(e: MouseEvent, p: ProjectInfo) {
    openMenu(e, [
      { label: "Open", action: () => onOpen(p.path) },
      { label: "Show in Finder", action: () => api.reveal(p.path) },
      { label: "Open PDF in Preview", disabled: !p.pdf, action: () => p.pdf && api.openExternal(p.pdf) },
      ...(!p.external && (folders.length || p.folder)
        ? (["sep", ...["", ...folders]
            .filter((f) => f !== p.folder)
            .map((f) => ({ label: f ? `Move to ${f.replace(/\//g, " › ")}` : "Move to Library (top level)", action: () => moveTo(p.path, f) }))] as MenuEntry[])
        : []),
      "sep",
      p.external
        ? {
            label: "Remove from Library",
            action: async () => {
              await api.forgetProject(p.path);
              refresh();
            },
          }
        : {
            label: "Move to Trash",
            danger: true,
            action: async () => {
              const ok = await ask(`Move “${p.name}” and all of its files to the Trash?`, {
                title: "Move to Trash",
                kind: "warning",
                okLabel: "Move to Trash",
              });
              if (!ok) return;
              try {
                await api.trashProject(p.path);
                refresh();
              } catch (err) {
                toast(errorText(err), "error");
              }
            },
          },
    ]);
  }

  function setView(v: string | null) {
    view = v;
    prefs.set("lib.view", v);
    sel = -1;
  }

  const counts = $derived.by(() => {
    const c: Record<string, number> = {};
    for (const p of projects) if (!p.external) c[p.folder] = (c[p.folder] ?? 0) + 1;
    return c;
  });

  async function moveTo(path: string, folder: string) {
    try {
      const moved = await api.moveProject(path, folder);
      const t = thumbUrls.get(path);
      if (t) {
        thumbUrls.delete(path);
        thumbUrls.set(moved.path, { mtime: moved.thumbMtime, url: t.url });
      }
      if (pages[path] != null) {
        pages = { ...pages, [moved.path]: pages[path] };
        prefs.set("lib.pages", pages);
      }
      toast(`Moved “${moved.name}” to ${folder ? folder.split("/").pop() : "Library"}`);
      refresh();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function commitFolder() {
    const e = folderEdit;
    folderEdit = null;
    if (!e || !e.value.trim()) return;
    try {
      if (e.mode === "new") {
        const made = await api.createFolder(e.target, e.value.trim());
        await refresh();
        setView(made);
      } else {
        const renamed = await api.renameFolder(e.target, e.value.trim());
        for (const p of [...thumbUrls.keys()]) {
          const old = `${root}/${e.target}/`;
          if (p.startsWith(old)) {
            const v = thumbUrls.get(p)!;
            thumbUrls.delete(p);
            thumbUrls.set(`${root}/${renamed}/${p.slice(old.length)}`, v);
          }
        }
        await refresh();
        if (view === e.target) setView(renamed);
      }
    } catch (err) {
      toast(errorText(err), "error");
    }
  }

  function folderMenu(ev: MouseEvent, f: string) {
    openMenu(ev, [
      { label: "New Folder Inside", action: () => (folderEdit = { mode: "new", target: f, value: "" }) },
      { label: "Rename", action: () => (folderEdit = { mode: "rename", target: f, value: f.split("/").pop()! }) },
      { label: "Show in Finder", action: () => api.reveal(`${root}/${f}`) },
      "sep",
      {
        label: "Move to Trash",
        danger: true,
        action: async () => {
          const inside = projects.filter((p) => p.folder === f || p.folder.startsWith(f + "/")).length;
          const ok = await ask(
            inside ? `Move “${f.split("/").pop()}” and the ${inside} ${inside === 1 ? "paper" : "papers"} in it to the Trash?` : `Move “${f.split("/").pop()}” to the Trash?`,
            { title: "Move to Trash", kind: "warning", okLabel: "Move to Trash" },
          );
          if (!ok) return;
          try {
            await api.trashFolder(f);
            if (view === f || view?.startsWith(f + "/")) setView(null);
            refresh();
          } catch (err) {
            toast(errorText(err), "error");
          }
        },
      },
    ]);
  }

  /* Drag a card onto a folder in the sidebar. */
  function cardDown(e: PointerEvent, p: ProjectInfo) {
    if (e.button !== 0 || p.external) return;
    const sx = e.clientX;
    const sy = e.clientY;
    const move = (ev: PointerEvent) => {
      if (!drag && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
      const target = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>("[data-drop]");
      drag = { path: p.path, name: p.name, x: ev.clientX, y: ev.clientY, over: target ? target.dataset.drop! : null };
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (drag) {
        suppressClick = true;
        setTimeout(() => (suppressClick = false), 0);
        const over = drag.over;
        drag = null;
        if (over !== null) moveTo(p.path, over === "/" ? "" : over);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function setSort(s: "recent" | "name") {
    sort = s;
    prefs.set("lib.sort", s);
  }

  function columns(): number {
    if (!gridEl) return 1;
    return Math.max(1, getComputedStyle(gridEl).gridTemplateColumns.split(" ").length);
  }

  async function move(d: number) {
    if (!shown.length) return;
    sel = sel < 0 ? 0 : Math.max(0, Math.min(shown.length - 1, sel + d));
    await tick();
    gridEl?.querySelector<HTMLElement>(".card.sel")?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (creating) return;
    const m = e.metaKey || e.ctrlKey;
    const inSearch = document.activeElement === searchEl;
    const k = e.key.toLowerCase();
    if (m && k === "n") {
      e.preventDefault();
      creating = true;
    } else if (m && k === "o") {
      e.preventDefault();
      openFolder();
    } else if ((m && k === "f") || (e.key === "/" && !inSearch)) {
      e.preventDefault();
      searchEl?.focus();
    } else if (e.key === "Escape" && inSearch) {
      query = "";
      searchEl?.blur();
    } else if (inSearch && e.key === "Enter" && shown.length) {
      onOpen(shown[0].path);
    } else if (!inSearch) {
      if (e.key === "ArrowRight") move(1);
      else if (e.key === "ArrowLeft") move(-1);
      else if (e.key === "ArrowDown") move(columns());
      else if (e.key === "ArrowUp") move(-columns());
      else return;
      e.preventDefault();
    }
  }

  let libTimer: ReturnType<typeof setTimeout> | null = null;
  let unlistenLib: UnlistenFn | null = null;

  onMount(async () => {
    refresh();
    window.addEventListener("focus", refresh);
    api.watchLibrary();
    unlistenLib = await listen("library-change", () => {
      if (libTimer) clearTimeout(libTimer);
      libTimer = setTimeout(refresh, 250);
    });
    unlisten = await getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") dropping = true;
      else if (p.type === "leave") dropping = false;
      else if (p.type === "drop") {
        dropping = false;
        const zip = p.paths.find((x) => x.toLowerCase().endsWith(".zip"));
        if (zip) importZip(zip);
        else if (p.paths[0]) {
          const t = p.paths[0];
          const target = t.toLowerCase().endsWith(".tex") ? t.slice(0, t.lastIndexOf("/")) : t;
          api.addFolder(target).then((x) => onOpen(x.path)).catch((err) => toast(errorText(err), "error"));
        }
      }
    });
  });

  onDestroy(() => {
    alive = false;
    window.removeEventListener("focus", refresh);
    unlisten?.();
    unlistenLib?.();
    if (libTimer) clearTimeout(libTimer);
  });

  function meta(p: ProjectInfo): string {
    const n = pages[p.path];
    const when = relTime(Math.max(p.modified, p.opened));
    return n ? `${n} ${n === 1 ? "page" : "pages"} · ${when}` : when;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="lib">
  <header class="toolbar" data-tauri-drag-region>
    <span class="tb-title" data-tauri-drag-region>Library</span>
    <div class="spacer" data-tauri-drag-region></div>
    <label class="search">
      <Search size={14} strokeWidth={2} />
      <input bind:this={searchEl} bind:value={query} placeholder="Search" spellcheck="false" autocomplete="off" />
      {#if !query}<kbd>{mod}F</kbd>{/if}
    </label>
    <button class="btn secondary" onclick={openFolder} title="Open a folder ({mod}O)">
      <FolderOpen size={15} strokeWidth={1.75} />Open Folder
    </button>
    <button class="btn secondary" onclick={() => importZip()} title="In Overleaf: Menu → Download → Source">
      <Upload size={15} strokeWidth={1.75} />Import .zip
    </button>
    <button class="btn primary" onclick={() => (creating = true)} title="New paper ({mod}N)">
      <Plus size={15} strokeWidth={2.25} />New Paper
    </button>
    <button class="icon-btn" onclick={libraryMenu} title="Library options"><Ellipsis size={16} strokeWidth={1.75} /></button>
  </header>

  <div class="split">
  <aside class="side">
    <div class="side-list">
      <button class="frow" class:active={view === null} class:over={drag?.over === "/"} data-drop="/" onclick={() => setView(null)}>
        <span class="fic"><Papers size={16} strokeWidth={1.75} /></span>
        <span class="fname">All Papers</span>
        <span class="fcount tnum">{projects.length}</span>
      </button>
    </div>
    <div class="side-head">
      <span>Folders</span>
      <button class="icon-btn" title="New folder" onclick={() => (folderEdit = { mode: "new", target: "", value: "" })}>
        <FolderPlus size={16} strokeWidth={1.75} />
      </button>
    </div>
    <div class="side-list">
      {#if folderEdit?.mode === "new" && folderEdit.target === ""}
        {@render folderInput(0)}
      {/if}
      {#each folders as f (f)}
        {@const depth = f.split("/").length - 1}
        {#if folderEdit?.mode === "rename" && folderEdit.target === f}
          {@render folderInput(depth)}
        {:else}
          <button
            class="frow"
            class:active={view === f}
            class:over={drag?.over === f}
            data-drop={f}
            style="--depth:{depth}"
            onclick={() => setView(f)}
            oncontextmenu={(e) => folderMenu(e, f)}
          >
            <span class="fic"><FolderIcon size={16} strokeWidth={1.75} /></span>
            <span class="fname">{f.split("/").pop()}</span>
            {#if counts[f]}<span class="fcount tnum">{counts[f]}</span>{/if}
          </button>
        {/if}
        {#if folderEdit?.mode === "new" && folderEdit.target === f}
          {@render folderInput(depth + 1)}
        {/if}
      {/each}
      {#if !folders.length && !folderEdit}
        <p class="side-hint">Group papers into folders. Drag a paper onto a folder to move it.</p>
      {/if}
    </div>
  </aside>

  <main>
    <div class="inner">
      <div class="head">
        <h1>{view === null ? "All Papers" : view.split("/").pop()}</h1>
        {#if projects.length > 1}
          <div class="seg" role="radiogroup" aria-label="Sort">
            <button role="radio" aria-checked={sort === "recent"} class:on={sort === "recent"} onclick={() => setSort("recent")}>Recent</button>
            <button role="radio" aria-checked={sort === "name"} class:on={sort === "name"} onclick={() => setSort("name")}>Name</button>
          </div>
        {/if}
      </div>

      {#if loaded && view !== null && !shown.length && !query}
        <div class="empty">
          <p class="em">This folder is empty</p>
          <p>Drag papers here from All Papers, or create a new one in this folder.</p>
          <div class="row">
            <button class="btn primary" onclick={() => (creating = true)}><Plus size={15} strokeWidth={2.25} />New Paper</button>
          </div>
        </div>
      {:else if loaded && !projects.length}
        <div class="empty">
          <p class="em">No papers yet</p>
          <p>Create one, or bring a project over from Overleaf with Menu → Download → Source and drop the .zip here.</p>
          <div class="row">
            <button class="btn primary" onclick={() => (creating = true)}><Plus size={15} strokeWidth={2.25} />New Paper</button>
            <button class="btn secondary" onclick={() => importZip()}><Upload size={15} strokeWidth={1.75} />Import .zip</button>
          </div>
        </div>
      {:else if loaded && !shown.length}
        <div class="empty"><p class="em">No papers match “{query}”</p></div>
      {:else}
        <div class="grid" bind:this={gridEl}>
          {#each shown as p, i (p.path)}
            <button
              class="card"
              class:sel={i === sel}
              onclick={() => !suppressClick && onOpen(p.path)}
              onpointerdown={(e) => cardDown(e, p)}
              onfocus={() => (sel = i)}
              oncontextmenu={(e) => cardMenu(e, p)}
            >
              <div class="sheet">
                {#if thumbs[p.path]}
                  <img src={thumbs[p.path]} alt="" draggable="false" />
                {:else}
                  <div class="cover">
                    <span class="ct">{p.title ?? p.name}</span>
                    {#if building === p.path}<span class="cs">Rendering preview…</span>{/if}
                  </div>
                {/if}
              </div>
              <span class="name">{p.name}</span>
              <span class="meta tnum">{meta(p)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </main>
  </div>

  {#if drag}
    <div class="ghost" style="left:{drag.x + 12}px;top:{drag.y + 12}px">
      {#if thumbs[drag.path]}<img src={thumbs[drag.path]} alt="" />{/if}
      <span>{drag.name}</span>
    </div>
  {/if}

  {#if dropping}
    <div class="drop"><div class="popover">Drop an Overleaf .zip or a project folder</div></div>
  {/if}
</div>

{#snippet folderInput(depth: number)}
  <div class="frow editing" style="--depth:{depth}">
    <span class="fic"><FolderIcon size={16} strokeWidth={1.75} /></span>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:value={folderEdit!.value}
      autofocus
      placeholder="Folder name"
      spellcheck="false"
      autocomplete="off"
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commitFolder();
        } else if (e.key === "Escape") {
          e.preventDefault();
          folderEdit = null;
        }
        e.stopPropagation();
      }}
      onblur={commitFolder}
    />
  </div>
{/snippet}

{#if creating}
  <NewPaper
    folder={view ?? ""}
    onClose={() => (creating = false)}
    onCreated={(p) => {
      creating = false;
      onOpen(p.path);
    }}
  />
{/if}

<style>
  .lib {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .toolbar {
    height: 44px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 84px;
    background: var(--bg-side);
    border-bottom: 1px solid var(--line);
  }
  .tb-title {
    font-weight: 600;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 220px;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text-3);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px transparent;
    transition: box-shadow 0.12s ease-out, background 0.12s ease-out;
  }
  .search:focus-within {
    background: var(--bg);
    box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 3px var(--accent-soft);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--text);
  }
  .search input::placeholder {
    color: var(--text-3);
  }

  .split {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .side {
    width: 220px;
    flex: none;
    overflow: auto;
    background: var(--bg-side);
    border-right: 1px solid var(--line);
    padding: 12px 0 24px;
  }
  .side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    margin: 16px 0 4px;
    padding: 0 8px 0 20px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .side-list {
    padding: 0 8px;
  }
  .frow {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px 0 calc(12px + var(--depth, 0) * 16px);
    border-radius: 6px;
    text-align: left;
  }
  .frow:hover {
    background: var(--hover);
  }
  .frow.active {
    background: var(--selected);
  }
  .frow.over {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .fic {
    display: grid;
    place-items: center;
    width: 16px;
    flex: none;
    color: var(--text-3);
  }
  .frow.active .fic {
    color: var(--accent);
  }
  .fname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fcount {
    font-size: 12px;
    color: var(--text-3);
  }
  .frow.editing input {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 6px;
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 4px;
  }
  .side-hint {
    margin: 4px 12px;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }
  .ghost {
    position: fixed;
    z-index: 95;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px 6px 6px;
    border-radius: 8px;
    background: var(--bg-raised);
    box-shadow: var(--shadow-pop);
    pointer-events: none;
    font-size: 12px;
    font-weight: 500;
    max-width: 260px;
  }
  .ghost img {
    width: 28px;
    height: 36px;
    object-fit: cover;
    object-position: top;
    border-radius: 2px;
    background: #fff;
  }
  .ghost span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  main {
    flex: 1;
    overflow: auto;
    min-width: 0;
  }
  .inner {
    max-width: 1240px;
    padding: 32px 32px 64px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
  }
  h1 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 28px;
    line-height: 34px;
    font-weight: 600;
    letter-spacing: -0.4px;
  }
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: 8px;
    background: var(--hover);
  }
  .seg button {
    height: 24px;
    padding: 0 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--bg-raised);
    color: var(--text);
    box-shadow: 0 0 0 0.5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, 0.15);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 32px 24px;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    text-align: left;
    min-width: 0;
    border-radius: 8px;
  }
  .card:focus-visible {
    outline: none;
  }
  .sheet {
    position: relative;
    aspect-ratio: 8.5 / 11;
    border-radius: 2px;
    background: #fff;
    box-shadow: var(--shadow-page);
    overflow: hidden;
    margin-bottom: 12px;
    transition: transform 0.12s ease-out, box-shadow 0.12s ease-out;
  }
  .card:hover .sheet {
    transform: translateY(-2px);
  }
  .card.sel .sheet,
  .card:focus-visible .sheet {
    box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--accent), var(--shadow-page);
  }
  .sheet img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
    object-position: top;
  }
  .cover {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 22% 14% 0;
    gap: 10px;
    text-align: center;
    color: #1b1b1f;
  }
  .ct {
    font-family: var(--font-serif);
    font-size: 13px;
    line-height: 17px;
    font-weight: 600;
  }
  .cs {
    font-size: 11px;
    color: #8a8a92;
  }
  .name {
    font-family: var(--font-serif);
    font-size: 15px;
    line-height: 20px;
    font-weight: 600;
    letter-spacing: -0.1px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta {
    margin-top: 2px;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-2);
  }

  .empty {
    max-width: 400px;
    margin: 96px auto 0;
    text-align: center;
    color: var(--text-2);
  }
  .empty p {
    margin: 0 0 4px;
  }
  .empty .em {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 8px;
  }
  .empty .row {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 20px;
  }
  .drop {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 60%, transparent);
    pointer-events: none;
  }
  .drop div {
    padding: 16px 24px;
  }
</style>
