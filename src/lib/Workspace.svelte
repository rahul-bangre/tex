<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { save as saveDialog, open as openDialog, ask } from "@tauri-apps/plugin-dialog";
  import { openSearchPanel } from "@codemirror/search";
  import { api, errorText, type CompileResult, type Diagnostic, type Entry, type ProjectInfo } from "./api";
  import { Editor, scanSymbols, outline, type OutlineItem } from "./editor";
  import PdfView from "./PdfView.svelte";
  import FileTree from "./FileTree.svelte";
  import Problems from "./Problems.svelte";
  import QuickOpen from "./QuickOpen.svelte";
  import { openMenu, prefs, toast, mod, type MenuEntry } from "./ui.svelte";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import Play from "@lucide/svelte/icons/play";
  import Download from "@lucide/svelte/icons/arrow-down-to-line";
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import Search from "@lucide/svelte/icons/search";
  import Crosshair from "@lucide/svelte/icons/locate-fixed";
  import FileText from "@lucide/svelte/icons/file-text";

  let { path, onBack }: { path: string; onBack: () => void } = $props();

  const TEXT_EXT =
    /\.(tex|bib|sty|cls|ltx|dtx|ins|bbx|cbx|def|cfg|clo|bst|txt|md|markdown|csv|tsv|dat|json|yaml|yml|toml|lua|py|r|m|sh|tikz|pgf|gitignore|latexmkrc|log|bbl)$/i;
  const IMAGE_EXT = /\.(png|jpe?g|gif|svg|webp|bmp)$/i;
  const SOURCE_EXT = /\.(tex|bib|sty|cls|bbx|cbx|def|cfg|png|jpe?g|pdf|eps|svg|tikz|pgf|csv|dat)$/i;

  let info = $state<ProjectInfo | null>(null);
  let entries = $state<Entry[]>([]);
  let main = $state<string | null>(null);
  let current = $state<string | null>(null);
  let view = $state<"text" | "image" | "binary" | "none">("none");
  let imageUrl = $state<string | null>(null);
  const dirty = new SvelteSet<string>();

  let status = $state<"idle" | "running" | "ok" | "warn" | "error" | "failed">("idle");
  let result = $state<CompileResult | null>(null);
  let pdfPath = $state<string | null>(null);
  let pdfVersion = $state(0);
  let auto = $state(prefs.get("auto", true));
  let showFiles = $state(prefs.get("files", true));
  let showProblems = $state(false);
  let showOutline = $state(prefs.get("outline", true));
  let split = $state(prefs.get("split", 0.5));
  let quick = $state(false);
  let dropping = $state(false);
  let tectonicMissing = $state(false);
  let outlineItems = $state<OutlineItem[]>([]);
  let cursorLine = $state(1);
  let treeEdit = $state<{ mode: "rename" | "file" | "folder"; target: string; value: string } | null>(null);

  let editorHost: HTMLDivElement;
  let mainEl: HTMLDivElement;
  let pdfView = $state<PdfView>();
  let editor: Editor;

  const saved = new Map<string, string>();
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const writes = new Map<string, Promise<void>>();
  const symbolIndex = { labels: new Map<string, Set<string>>(), cites: new Map<string, Set<string>>() };
  let sourceVersion = 0;
  let builtVersion = -1;
  let compiling = false;
  let queued: { full: boolean } | null = null;
  let followups = 0;
  let compileTimer: ReturnType<typeof setTimeout> | null = null;
  let idleWaiters: (() => void)[] = [];
  let unlisten: UnlistenFn[] = [];
  let destroyed = false;

  const texFiles = $derived(entries.filter((e) => !e.dir && e.path.endsWith(".tex")).map((e) => e.path));
  const errors = $derived(result?.diagnostics.filter((d) => d.severity === "error") ?? []);
  const warnings = $derived(result?.diagnostics.filter((d) => d.severity === "warning") ?? []);
  const activeSection = $derived.by(() => {
    let idx = -1;
    for (let i = 0; i < outlineItems.length; i++) if (outlineItems[i].line <= cursorLine) idx = i;
    return idx;
  });

  /* ── Files ─────────────────────────────────────────────── */

  async function openFile(rel: string, line?: number) {
    if (current && current !== rel) await flushSave(current);
    if (IMAGE_EXT.test(rel)) {
      try {
        const buf = await api.readBytes(path, rel);
        const type = rel.toLowerCase().endsWith(".svg") ? "image/svg+xml" : "";
        if (imageUrl) URL.revokeObjectURL(imageUrl);
        imageUrl = URL.createObjectURL(new Blob([buf], type ? { type } : {}));
        current = rel;
        view = "image";
      } catch (e) {
        toast(errorText(e), "error");
      }
    } else if (TEXT_EXT.test(rel) || !/\.[a-z0-9]+$/i.test(rel)) {
      try {
        if (!editor.has(rel)) {
          const text = await api.readText(path, rel);
          saved.set(rel, text);
          editor.open(rel, text);
        } else {
          editor.open(rel, "");
        }
        current = rel;
        view = "text";
        cursorLine = editor.cursorLine();
        refreshOutline();
        applyDiagnostics();
        if (line) editor.gotoLine(line);
      } catch (e) {
        toast(errorText(e), "error");
        return;
      }
    } else {
      current = rel;
      view = "binary";
    }
    prefs.set("last:" + path, rel);
  }

  function refreshOutline() {
    outlineItems = view === "text" && current ? outline(editor.text(current) ?? "") : [];
  }

  let outlineTimer: ReturnType<typeof setTimeout> | null = null;

  function onChange(rel: string) {
    dirty.add(rel);
    const t = saveTimers.get(rel);
    if (t) clearTimeout(t);
    saveTimers.set(rel, setTimeout(() => flushSave(rel), 250));
    // Typing again postpones a pending auto-compile.
    if (compileTimer) clearTimeout(compileTimer);
    if (outlineTimer) clearTimeout(outlineTimer);
    outlineTimer = setTimeout(refreshOutline, 250);
  }

  async function flushSave(rel: string) {
    const t = saveTimers.get(rel);
    if (t) {
      clearTimeout(t);
      saveTimers.delete(rel);
    }
    const prev = writes.get(rel);
    if (prev) await prev;
    const text = editor.text(rel);
    if (text == null) return;
    if (text === saved.get(rel)) {
      if (!saveTimers.has(rel)) dirty.delete(rel);
      return;
    }
    const p = (async () => {
      try {
        await api.writeText(path, rel, text);
        saved.set(rel, text);
        sourceVersion++;
        scanSymbols(rel, text, symbolIndex);
        if (editor.text(rel) === text) dirty.delete(rel);
        if (auto) scheduleCompile(150);
      } catch (e) {
        toast(`Couldn't save ${rel}: ${errorText(e)}`, "error");
      }
    })();
    writes.set(rel, p);
    await p;
    if (writes.get(rel) === p) writes.delete(rel);
  }

  async function flushAll() {
    await Promise.all([...new Set([...saveTimers.keys(), ...dirty])].map((r) => flushSave(r)));
  }

  async function refreshTree() {
    try {
      entries = await api.listTree(path);
    } catch {
      /* folder may be gone */
    }
  }

  async function indexSymbols() {
    const files = entries.filter((e) => !e.dir && /\.(tex|bib)$/.test(e.path) && e.size < 2_000_000).slice(0, 300);
    for (const f of files) {
      if (destroyed) return;
      if (symbolIndex.labels.has(f.path) || symbolIndex.cites.has(f.path)) continue;
      try {
        scanSymbols(f.path, await api.readText(path, f.path), symbolIndex);
      } catch {
        /* skip */
      }
    }
  }

  function getSymbols() {
    const labels = new Set<string>();
    const cites = new Set<string>();
    for (const s of symbolIndex.labels.values()) s.forEach((x) => labels.add(x));
    for (const s of symbolIndex.cites.values()) s.forEach((x) => cites.add(x));
    return { labels, cites };
  }

  /* ── Compile ───────────────────────────────────────────── */

  function scheduleCompile(ms: number) {
    if (compileTimer) clearTimeout(compileTimer);
    compileTimer = setTimeout(() => {
      compileTimer = null;
      compile({ auto: true });
    }, ms);
  }

  async function compile(opts: { full?: boolean; followup?: boolean; auto?: boolean } = {}) {
    if (!main) {
      toast("No main file. Right-click a .tex file and choose Set as main file.");
      return;
    }
    if (compiling) {
      queued = { full: (queued?.full ?? false) || !!opts.full };
      return;
    }
    if (!opts.followup) followups = 0;
    compiling = true;
    status = "running";
    await flushAll();
    builtVersion = sourceVersion;
    let r: CompileResult;
    try {
      r = await api.compile(path, main, !!opts.full);
      tectonicMissing = false;
    } catch (e) {
      const msg = errorText(e);
      tectonicMissing = msg.includes("Tectonic isn't installed");
      r = { ok: false, pdf: null, ms: 0, needsRerun: false, diagnostics: [{ severity: "error", file: null, line: null, message: msg }], output: "" };
    }
    compiling = false;
    if (destroyed) return;
    if (r.pdf) {
      pdfPath = r.pdf;
      pdfVersion++;
    }
    // A single fast pass can leave references one step behind; settle them quietly.
    const settle = r.needsRerun && followups < 2 && !queued && sourceVersion === builtVersion;
    if (settle) {
      followups++;
      queued = { full: false };
    }
    if (!settle || !r.pdf) {
      result = r;
      const errs = r.diagnostics.filter((d) => d.severity === "error").length;
      const warns = r.diagnostics.filter((d) => d.severity === "warning").length;
      status = !r.pdf ? "failed" : errs ? "error" : warns ? "warn" : "ok";
      // Pop the panel open only when you asked for the build; never while you type.
      if (!r.pdf && !opts.auto && !opts.followup) showProblems = true;
      applyDiagnostics();
    }
    if (queued) {
      const q = queued;
      queued = null;
      compile({ full: q.full, followup: settle, auto: opts.auto });
    } else {
      idleWaiters.forEach((f) => f());
      idleWaiters = [];
    }
  }

  function waitIdle(): Promise<void> {
    if (!compiling && !queued) return Promise.resolve();
    return new Promise((res) => idleWaiters.push(res));
  }

  function applyDiagnostics() {
    if (!editor || view !== "text" || !current) return;
    const ds = (result?.diagnostics ?? [])
      .filter((d) => d.file === current && d.line && d.severity !== "box")
      .map((d) => ({ line: d.line!, severity: d.severity as "error" | "warning", message: d.message }));
    editor.setDiagnostics(ds);
  }

  async function ensureFresh() {
    await flushAll();
    if (sourceVersion !== builtVersion || compiling || queued) {
      if (!compiling) compile();
      await waitIdle();
    }
  }

  async function exportPdf() {
    await ensureFresh();
    if (!pdfPath) {
      toast("No PDF yet. Fix the errors in Problems and compile again.", "error");
      return;
    }
    const base = (info?.name ?? "paper").replace(/[/\\:]/g, "-");
    const dir = prefs.get("exportDir", "");
    const dest = await saveDialog({
      defaultPath: dir ? `${dir}/${base}.pdf` : `${base}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!dest) return;
    try {
      await api.exportPdf(pdfPath, dest);
      prefs.set("exportDir", dest.slice(0, dest.lastIndexOf("/")));
      toast(`wrote ${dest.replace(/^\/Users\/[^/]+/, "~")}`);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  function compileMenu(e: MouseEvent) {
    openMenu(e, [
      { label: "Compile", hint: `${mod}↵`, action: () => compile() },
      { label: "Recompile from scratch", hint: `${mod}⇧↵`, action: () => compile({ full: true }) },
    ]);
  }

  /* ── SyncTeX ───────────────────────────────────────────── */

  async function inverse(page: number, x: number, y: number) {
    if (!main) return;
    const hit = await api.synctexInverse(path, main, page, x, y);
    if (!hit) return;
    await openFile(hit.rel);
    if (view === "text") editor.gotoLine(hit.line);
  }

  async function forward() {
    if (!main || !current || view !== "text" || !pdfView) return;
    await ensureFresh();
    const r = await api.synctexForward(path, main, current, editor.cursorLine());
    if (r) pdfView.reveal(r);
    else toast("That line doesn't appear in the PDF.");
  }

  function goDiagnostic(d: Diagnostic) {
    if (d.file) openFile(d.file, d.line ?? undefined);
  }

  /* ── Tree actions ──────────────────────────────────────── */

  async function commitEdit(e: { mode: "rename" | "file" | "folder"; target: string; value: string }) {
    const name = e.value.replace(/^\/+|\/+$/g, "");
    try {
      if (e.mode === "rename") {
        const dir = e.target.includes("/") ? e.target.slice(0, e.target.lastIndexOf("/") + 1) : "";
        const to = dir + name;
        if (to === e.target) return;
        await flushSave(e.target);
        await api.renameEntry(path, e.target, to);
        editor.rename(e.target, to);
        const s = saved.get(e.target);
        if (s != null) {
          saved.delete(e.target);
          saved.set(to, s);
        }
        if (current === e.target) {
          current = to;
          prefs.set("last:" + path, to);
        }
        if (main === e.target) setMain(to);
      } else {
        const rel = (e.target ? e.target + "/" : "") + name;
        await api.createEntry(path, rel, e.mode === "folder");
        await refreshTree();
        if (e.mode === "file") openFile(rel);
      }
      await refreshTree();
    } catch (err) {
      toast(errorText(err), "error");
    }
  }

  async function deleteEntry(rel: string, dir: boolean) {
    const ok = await ask(`Move “${rel}” to the Trash?`, { title: "Move to Trash", kind: "warning", okLabel: "Move to Trash" });
    if (!ok) return;
    try {
      await api.trashEntry(path, rel);
      const gone = (p: string) => p === rel || (dir && p.startsWith(rel + "/"));
      for (const k of [...saved.keys()])
        if (gone(k)) {
          saved.delete(k);
          editor.forget(k);
          dirty.delete(k);
        }
      if (current && gone(current)) {
        current = null;
        view = "none";
        if (main && !gone(main)) openFile(main);
      }
      await refreshTree();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  function setMain(rel: string) {
    main = rel;
    api.setMain(path, rel);
    compile();
  }

  async function importIntoProject(paths: string[]) {
    try {
      const added = await api.importFiles(path, "", paths);
      await refreshTree();
      toast(added.length === 1 ? `added ${added[0]}` : `added ${added.length} files`);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function uploadFiles() {
    const picked = await openDialog({ multiple: true, directory: false, title: "Add files to this paper" });
    if (!picked) return;
    importIntoProject(Array.isArray(picked) ? picked : [picked]);
  }

  function mainMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuEntry[] = texFiles.map((f) => ({
      label: (f === main ? "● " : "   ") + f,
      action: () => f !== main && setMain(f),
    }));
    if (!items.length) items.push({ label: "No .tex files", disabled: true, action: () => {} });
    openMenu({ clientX: r.left, clientY: r.bottom + 2 }, items);
  }

  /* ── External changes ──────────────────────────────────── */

  let treeTimer: ReturnType<typeof setTimeout> | null = null;

  async function onFsChange(paths: string[]) {
    if (treeTimer) clearTimeout(treeTimer);
    treeTimer = setTimeout(refreshTree, 120);
    let external = false;
    for (const rel of paths) {
      if (saved.has(rel)) {
        if (dirty.has(rel) || writes.has(rel)) continue;
        let disk: string;
        try {
          disk = await api.readText(path, rel);
        } catch {
          continue;
        }
        if (disk !== saved.get(rel)) {
          saved.set(rel, disk);
          editor.replace(rel, disk);
          scanSymbols(rel, disk, symbolIndex);
          if (rel === current) refreshOutline();
          external = true;
        }
      } else if (SOURCE_EXT.test(rel)) {
        external = true;
        if (/\.(tex|bib)$/.test(rel)) api.readText(path, rel).then((t) => scanSymbols(rel, t, symbolIndex)).catch(() => {});
        if (rel === current && view === "image") openFile(rel);
      }
    }
    if (external) {
      sourceVersion++;
      if (auto) scheduleCompile(200);
    }
  }

  /* ── Layout ────────────────────────────────────────────── */

  function startResize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const rect = mainEl.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      split = Math.min(0.78, Math.max(0.22, (ev.clientX - rect.left) / rect.width));
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      prefs.set("split", split);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  function toggleFiles() {
    showFiles = !showFiles;
    prefs.set("files", showFiles);
  }

  function toggleAuto() {
    auto = !auto;
    prefs.set("auto", auto);
    if (auto && sourceVersion !== builtVersion) compile();
  }

  async function back() {
    await flushAll();
    onBack();
  }

  function onKey(e: KeyboardEvent) {
    const m = e.metaKey || e.ctrlKey;
    if (!m) return;
    const k = e.key.toLowerCase();
    let handled = true;
    if (k === "s" && !e.shiftKey) flushAll().then(() => compile());
    else if (k === "enter") compile({ full: e.shiftKey });
    else if (k === "e" && !e.shiftKey) exportPdf();
    else if (k === "p" && !e.shiftKey) quick = true;
    else if (k === "b" && !e.shiftKey) toggleFiles();
    else if (k === "m" && e.shiftKey) showProblems = !showProblems;
    else if (k === "j" && !e.shiftKey) forward();
    else if (k === "=" || k === "+") pdfView?.zoom(1);
    else if (k === "-") pdfView?.zoom(-1);
    else if (k === "0") pdfView?.zoom(0);
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  onMount(async () => {
    editor = new Editor(editorHost, {
      onChange,
      onCursor: (l) => (cursorLine = l),
      getSymbols,
      keys: [],
    });

    if (import.meta.env.DEV) (window as any).__tex = { editor, openFile, compile, forward, inverse, get result() { return result; }, get status() { return status; } };
    unlisten.push(await listen<string[]>("fs-change", (e) => onFsChange(e.payload)));
    unlisten.push(
      await getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "over" || p.type === "enter") dropping = true;
        else if (p.type === "leave") dropping = false;
        else if (p.type === "drop") {
          dropping = false;
          if (p.paths.length) importIntoProject(p.paths);
        }
      }),
    );

    try {
      const opened = await api.openProject(path);
      info = opened.info;
      entries = opened.tree;
      main = opened.info.main;
      if (opened.info.pdf) {
        // Show the last build at once; a fresh one follows.
        pdfPath = opened.info.pdf;
        pdfVersion++;
      }
    } catch (e) {
      toast(errorText(e), "error");
      onBack();
      return;
    }

    const last = prefs.get<string | null>("last:" + path, null);
    const start = last && entries.some((e) => e.path === last) ? last : main;
    if (start) await openFile(start);
    if (main) compile();
    indexSymbols();
  });

  onDestroy(() => {
    destroyed = true;
    flushAll();
    unlisten.forEach((u) => u());
    editor?.destroy();
    if (imageUrl) URL.revokeObjectURL(imageUrl);
    api.closeProject();
  });

  const statusText = $derived.by(() => {
    switch (status) {
      case "running":
        return "Compiling…";
      case "ok":
        return `Built in ${((result?.ms ?? 0) / 1000).toFixed(1)}s`;
      case "warn":
        return `${warnings.length} warning${warnings.length === 1 ? "" : "s"}`;
      case "error":
        return `${errors.length} error${errors.length === 1 ? "" : "s"}`;
      case "failed":
        return "Build failed";
      default:
        return "Not built";
    }
  });
</script>

<svelte:window onkeydowncapture={onKey} />

<div class="ws">
  <header class="toolbar" data-tauri-drag-region>
    <button class="icon-btn" onclick={back} title="Library"><ChevronLeft size={18} strokeWidth={1.75} /></button>
    <button class="icon-btn" class:on={showFiles} onclick={toggleFiles} title="Show sidebar ({mod}B)">
      <PanelLeft size={16} strokeWidth={1.75} />
    </button>
    <div class="crumb" data-tauri-drag-region>
      <span class="pname" data-tauri-drag-region title={info?.title ?? ""}>{info?.name ?? ""}</span>
      {#if main}
        <button class="mainfile" title="Main file" onclick={mainMenu}>
          <span>{main}</span><ChevronDown size={12} strokeWidth={2} />
        </button>
      {/if}
    </div>
    <div class="spacer" data-tauri-drag-region></div>
    <button class="status {status}" onclick={() => (showProblems = !showProblems)} title="Problems ({mod}⇧M)">
      <span class="dot"></span><span class="tnum">{statusText}</span>
    </button>
    <button class="switch" class:on={auto} onclick={toggleAuto} title="Compile automatically when you pause typing" role="switch" aria-checked={auto}>
      <span class="track"><span class="knob"></span></span>Auto
    </button>
    <button class="btn secondary" onclick={exportPdf} title="Save PDF as… ({mod}E)">
      <Download size={15} strokeWidth={1.75} />Export
    </button>
    <button class="btn primary" onclick={() => compile()} oncontextmenu={compileMenu} title="Compile ({mod}↵). Right-click for a clean rebuild.">
      <Play size={13} strokeWidth={2.25} fill="currentColor" />Compile
    </button>
  </header>

  {#if tectonicMissing}
    <div class="banner">
      tex typesets with Tectonic, which isn't installed. Run <code>brew install tectonic</code> in Terminal, then compile.
    </div>
  {/if}

  <div class="body">
    {#if showFiles}
      <aside class="side">
        <div class="pane-head">
          <span class="title">Files</span>
          <span class="spacer"></span>
          <button class="icon-btn" title="New file" onclick={() => (treeEdit = { mode: "file", target: "", value: "" })}>
            <FilePlus size={16} strokeWidth={1.75} />
          </button>
          <button class="icon-btn" title="New folder" onclick={() => (treeEdit = { mode: "folder", target: "", value: "" })}>
            <FolderPlus size={16} strokeWidth={1.75} />
          </button>
          <button class="icon-btn" title="Add files…" onclick={uploadFiles}><Upload size={16} strokeWidth={1.75} /></button>
        </div>
        <div class="tree-scroll">
          <FileTree
            {entries}
            {current}
            {main}
            {dirty}
            projectKey={path}
            bind:edit={treeEdit}
            onOpen={(r) => openFile(r)}
            onCommit={commitEdit}
            onDelete={deleteEntry}
            onSetMain={setMain}
            onReveal={(r) => api.reveal(path + "/" + r)}
          />
        </div>
        {#if view === "text" && outlineItems.length}
          <div class="outline" class:open={showOutline}>
            <button
              class="pane-head outline-head"
              onclick={() => {
                showOutline = !showOutline;
                prefs.set("outline", showOutline);
              }}
            >
              <span class="title">Outline</span>
              <span class="spacer"></span>
              <span class="chev" class:open={showOutline}><ChevronDown size={14} strokeWidth={2} /></span>
            </button>
            {#if showOutline}
              <div class="outline-list">
                {#each outlineItems as o, i}
                  <button
                    class="o-item"
                    class:active={i === activeSection}
                    style="--lvl:{Math.min(3, Math.max(0, o.level - 2))}"
                    onclick={() => editor.gotoLine(o.line)}
                    title={o.title}
                  >
                    {o.title}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </aside>
    {/if}

    <div class="main" bind:this={mainEl} style="--split:{split}">
      <section class="editor-col" style="flex-basis:calc(var(--split) * 100%)">
        <div class="pane-head">
          <span class="file-ic"><FileText size={14} strokeWidth={1.75} /></span>
          <span class="fname">{current ?? ""}</span>
          {#if current && dirty.has(current)}<span class="dirty" title="Saving"></span>{/if}
          <span class="spacer"></span>
          {#if view === "text"}
            <span class="ln tnum">Ln {cursorLine}</span>
            <button class="icon-btn" onclick={() => openSearchPanel(editor.view)} title="Find and replace ({mod}F)">
              <Search size={15} strokeWidth={1.75} />
            </button>
            <button class="icon-btn" onclick={forward} title="Show this line in the PDF ({mod}J)">
              <Crosshair size={15} strokeWidth={1.75} />
            </button>
          {/if}
        </div>
        <div class="editor-wrap">
          <div class="editor-host" class:hidden={view !== "text"} bind:this={editorHost}></div>
          {#if view === "image" && imageUrl}
            <div class="viewer">
              <img src={imageUrl} alt={current} />
            </div>
          {:else if view === "binary"}
            <div class="viewer">
              <p class="note">This file type can't be edited in tex.</p>
              <button class="btn secondary" onclick={() => current && api.openExternal(path + "/" + current)}>Open in Default App</button>
            </div>
          {:else if view === "none"}
            <div class="viewer"><p class="note">Select a file in the sidebar.</p></div>
          {/if}
        </div>
        {#if showProblems}
          <div class="problems">
            <Problems
              diagnostics={result?.diagnostics ?? []}
              output={result?.output ?? ""}
              onGo={goDiagnostic}
              onClose={() => (showProblems = false)}
            />
          </div>
        {/if}
      </section>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="divider" onpointerdown={startResize}></div>

      <section class="pdf-col">
        <PdfView bind:this={pdfView} path={pdfPath} version={pdfVersion} onInverse={inverse}>
          {#snippet empty()}
            <div class="pdf-empty">
              {#if status === "running"}
                <p class="em">Compiling {main}…</p>
                <p>The first build downloads the LaTeX packages this paper uses.</p>
              {:else if status === "failed"}
                <p class="em">No PDF was produced</p>
                <p>Open Problems to see what stopped the build.</p>
              {:else}
                <p class="em">No PDF yet</p>
                <p>Press {mod}↵ to compile.</p>
              {/if}
            </div>
          {/snippet}
        </PdfView>
      </section>
    </div>
  </div>

  {#if dropping}
    <div class="drop"><div class="popover">Drop files to copy them into <strong>{info?.name}</strong></div></div>
  {/if}
</div>

{#if quick}
  <QuickOpen
    files={entries.filter((e) => !e.dir).map((e) => e.path)}
    onPick={(f) => {
      quick = false;
      openFile(f);
    }}
    onClose={() => {
      quick = false;
      if (view === "text") editor.view.focus();
    }}
  />
{/if}

<style>
  .ws {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .toolbar {
    height: 44px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 80px;
    background: var(--bg-side);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
    min-width: 0;
  }
  .toolbar > .icon-btn {
    margin-right: -4px;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-left: 8px;
  }
  .pname {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 40px;
  }
  .mainfile {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 6px 0 8px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--line);
    flex: none;
  }
  .mainfile:hover {
    background: var(--hover);
    color: var(--text);
  }
  .spacer {
    flex: 1;
    min-width: 12px;
    align-self: stretch;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--text-2);
    flex: none;
  }
  .status:hover {
    background: var(--hover);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
    flex: none;
  }
  .status.ok .dot {
    background: var(--ok);
  }
  .status.warn .dot {
    background: var(--warn);
  }
  .status.error .dot,
  .status.failed .dot {
    background: var(--err);
  }
  .status.error,
  .status.failed {
    color: var(--err);
  }
  .status.running .dot {
    background: var(--accent);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.25;
    }
  }

  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--text-2);
    flex: none;
  }
  .switch:hover {
    background: var(--hover);
    color: var(--text);
  }
  .track {
    position: relative;
    width: 26px;
    height: 16px;
    border-radius: 8px;
    background: var(--line-strong);
    transition: background 0.12s ease-out;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
    transition: transform 0.12s ease-out;
  }
  .switch.on .track {
    background: var(--accent);
  }
  .switch.on .knob {
    transform: translateX(10px);
  }

  .banner {
    padding: 8px 16px;
    font-size: 12px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--line);
  }
  .banner code {
    font-family: var(--font-mono);
    user-select: text;
    -webkit-user-select: text;
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .side {
    width: 240px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-side);
    border-right: 1px solid var(--line);
    min-height: 0;
  }
  .tree-scroll {
    flex: 1;
    overflow: auto;
    min-height: 84px;
  }
  .outline {
    flex: none;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line);
  }
  .outline.open {
    max-height: 45%;
  }
  .outline-head {
    width: 100%;
    text-align: left;
  }
  .outline-head:hover .title {
    color: var(--text);
  }
  .chev {
    display: grid;
    place-items: center;
    width: 28px;
    color: var(--text-3);
    transform: rotate(-90deg);
    transition: transform 0.12s ease-out;
  }
  .chev.open {
    transform: none;
  }
  .outline-list {
    overflow: auto;
    padding: 4px 8px 12px;
  }
  .o-item {
    display: block;
    width: 100%;
    height: 28px;
    text-align: left;
    padding: 0 8px 0 calc(8px + var(--lvl) * 16px);
    border-radius: 6px;
    font-size: 13px;
    line-height: 28px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .o-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .o-item.active {
    background: var(--selected);
    color: var(--text);
  }

  .main {
    flex: 1;
    display: flex;
    min-width: 0;
  }
  .editor-col {
    display: flex;
    flex-direction: column;
    flex-grow: 0;
    flex-shrink: 0;
    min-width: 320px;
    background: var(--bg);
  }
  .file-ic {
    display: grid;
    color: var(--text-3);
  }
  .fname {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    margin-left: 4px;
  }
  .dirty {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
    margin-left: 6px;
    flex: none;
  }
  .ln {
    font-size: 12px;
    color: var(--text-3);
    margin-right: 4px;
  }
  .editor-wrap {
    flex: 1;
    position: relative;
    min-height: 0;
  }
  .editor-host {
    position: absolute;
    inset: 0;
  }
  .editor-host.hidden {
    visibility: hidden;
  }
  .viewer {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px;
    background: var(--bg);
  }
  .viewer img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 2px;
    box-shadow: var(--shadow-page);
    background: #fff;
  }
  .note {
    margin: 0;
    color: var(--text-2);
  }
  .problems {
    height: 220px;
    flex: none;
  }
  .divider {
    width: 9px;
    margin: 0 -4px;
    z-index: 5;
    cursor: col-resize;
    flex: none;
    position: relative;
  }
  .divider::after {
    content: "";
    position: absolute;
    left: 4px;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--line);
    transition: background 0.12s ease-out;
  }
  .divider:hover::after {
    background: var(--accent);
  }
  .pdf-col {
    flex: 1;
    min-width: 280px;
  }
  .pdf-empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 32px;
    text-align: center;
    color: var(--text-2);
    font-size: 12px;
  }
  .pdf-empty p {
    margin: 0;
    max-width: 300px;
  }
  .pdf-empty .em {
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
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
