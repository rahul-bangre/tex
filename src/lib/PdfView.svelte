<script lang="ts">
  import { onDestroy, onMount, tick, type Snippet } from "svelte";
  import { loadPdf, destroyDoc, type PDFDoc, type PDFPage } from "./pdf";
  import { api, errorText, type Rect } from "./api";
  import { prefs, mod } from "./ui.svelte";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import MoveHorizontal from "@lucide/svelte/icons/move-horizontal";
  import Moon from "@lucide/svelte/icons/moon";

  let {
    path,
    version,
    onInverse,
    empty,
  }: {
    path: string | null;
    version: number;
    onInverse: (page: number, x: number, y: number) => void;
    empty?: Snippet;
  } = $props();

  const GAP = 16;
  const PAD = 24;

  let scroller: HTMLDivElement;
  let sizes = $state<{ w: number; h: number }[]>([]);
  let scale = $state(1);
  let fit = $state(prefs.get("pdf.fit", true));
  let night = $state(prefs.get("pdf.night", false));
  let pageNum = $state(1);
  let loadError = $state<string | null>(null);
  let flash = $state<(Rect & { key: number }) | null>(null);

  let doc: PDFDoc | null = null;
  let pages: PDFPage[] = [];
  let docGen = 0;
  let loadGen = 0;
  const pageEls: HTMLDivElement[] = [];
  const rendered = new Map<number, { key: string; canvas: HTMLCanvasElement }>();
  const inflight = new Map<number, { key: string; cancel: () => void }>();

  const maxW = $derived(sizes.reduce((m, s) => Math.max(m, s.w), 0));
  const tops = $derived.by(() => {
    const out: number[] = [];
    let y = PAD;
    for (const s of sizes) {
      out.push(y);
      y += s.h * scale + GAP;
    }
    return out;
  });

  $effect(() => {
    // Reload whenever a new build lands.
    void version;
    if (path) load(path);
    else clear();
  });

  function clear() {
    loadGen++;
    for (const t of inflight.values()) t.cancel();
    inflight.clear();
    rendered.clear();
    sizes = [];
    destroyDoc(doc);
    doc = null;
    pages = [];
  }

  async function load(p: string) {
    const my = ++loadGen;
    try {
      const buf = await api.readPdf(p);
      if (my !== loadGen) return;
      const d = await loadPdf(buf);
      if (my !== loadGen) {
        destroyDoc(d);
        return;
      }
      const ps = await Promise.all(Array.from({ length: d.numPages }, (_, i) => d.getPage(i + 1)));
      if (my !== loadGen) {
        destroyDoc(d);
        return;
      }
      const ss = ps.map((pg) => {
        const v = pg.getViewport({ scale: 1 });
        return { w: v.width, h: v.height };
      });
      const old = doc;
      for (const t of inflight.values()) t.cancel();
      inflight.clear();
      doc = d;
      pages = ps;
      docGen++;
      loadError = null;
      const changed = ss.length !== sizes.length || ss.some((s, i) => s.w !== sizes[i]?.w || s.h !== sizes[i]?.h);
      if (changed) sizes = ss;
      if (fit) applyFit();
      await tick();
      // Old canvases stay up until their replacements are drawn.
      for (const k of [...rendered.keys()]) if (k >= ss.length) rendered.delete(k);
      await renderVisible();
      destroyDoc(old);
    } catch (e) {
      if (my === loadGen) loadError = errorText(e);
    }
  }

  function applyFit() {
    if (!scroller || !maxW) return;
    const avail = scroller.clientWidth - PAD * 2;
    setScale(Math.max(0.2, avail / maxW), false);
  }

  let renderTimer: ReturnType<typeof setTimeout> | null = null;

  function setScale(next: number, anchor = true) {
    next = Math.min(5, Math.max(0.2, next));
    if (Math.abs(next - scale) < 0.001) return;
    if (anchor && scroller && sizes.length) {
      // Keep the point at the viewport centre fixed while zooming.
      const cy = scroller.scrollTop + scroller.clientHeight / 2;
      const i = pageAt(cy);
      const frac = (cy - tops[i]) / (sizes[i].h * scale);
      const cxFrac = (scroller.scrollLeft + scroller.clientWidth / 2) / Math.max(1, scroller.scrollWidth);
      scale = next;
      tick().then(() => {
        scroller.scrollTop = tops[i] + frac * sizes[i].h * scale - scroller.clientHeight / 2;
        scroller.scrollLeft = cxFrac * scroller.scrollWidth - scroller.clientWidth / 2;
      });
    } else {
      scale = next;
    }
    if (renderTimer) clearTimeout(renderTimer);
    renderTimer = setTimeout(() => renderVisible(), 110);
  }

  function pageAt(y: number): number {
    let lo = 0;
    let hi = tops.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (tops[mid] <= y) lo = mid;
      else hi = mid - 1;
    }
    return Math.max(0, lo);
  }

  async function renderPage(i: number) {
    const page = pages[i];
    const host = pageEls[i];
    if (!page || !host) return;
    const dpr = window.devicePixelRatio || 1;
    const key = `${docGen}:${scale.toFixed(4)}:${dpr}`;
    if (rendered.get(i)?.key === key) return;
    if (inflight.get(i)?.key === key) return;
    inflight.get(i)?.cancel();

    const vp = page.getViewport({ scale: scale * dpr });
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(vp.width);
    canvas.height = Math.floor(vp.height);
    // Timer-paced ("print") rendering keeps working while the window is in the background,
    // where macOS pauses requestAnimationFrame.
    const task = page.render({ canvas, viewport: vp, intent: "print" });
    let cancelled = false;
    inflight.set(i, {
      key,
      cancel: () => {
        cancelled = true;
        task.cancel();
      },
    });
    try {
      await task.promise;
    } catch {
      return;
    } finally {
      if (inflight.get(i)?.key === key) inflight.delete(i);
    }
    if (cancelled || !pageEls[i]) return;
    const prev = rendered.get(i)?.canvas;
    pageEls[i].prepend(canvas);
    prev?.remove();
    rendered.set(i, { key, canvas });
  }

  async function renderVisible() {
    if (!scroller || !sizes.length) return;
    const top = scroller.scrollTop;
    const h = scroller.clientHeight;
    const first = pageAt(top - h * 0.5);
    const last = pageAt(top + h * 1.5);
    const center = pageAt(top + h / 2);
    const order: number[] = [];
    for (let i = first; i <= last; i++) order.push(i);
    order.sort((a, b) => Math.abs(a - center) - Math.abs(b - center));
    // Free far-away pages.
    for (const [i, r] of rendered) {
      if (i < pageAt(top - h * 3) || i > pageAt(top + h * 4)) {
        r.canvas.remove();
        rendered.delete(i);
      }
    }
    for (const i of order) await renderPage(i);
  }

  let rafPending = false;
  function onScroll() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      if (!sizes.length) return;
      pageNum = pageAt(scroller.scrollTop + scroller.clientHeight * 0.3) + 1;
      renderVisible();
    });
  }

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    setFit(false);
    setScale(scale * Math.exp(-e.deltaY * (e.ctrlKey ? 0.012 : 0.004)));
  }

  function setFit(v: boolean) {
    fit = v;
    prefs.set("pdf.fit", v);
    if (v) applyFit();
  }

  export function zoom(dir: 1 | -1 | 0) {
    if (dir === 0) return setFit(true);
    setFit(false);
    const steps = [0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 5];
    const cur = scale;
    const next = dir > 0 ? steps.find((s) => s > cur + 0.01) ?? 5 : [...steps].reverse().find((s) => s < cur - 0.01) ?? 0.25;
    setScale(next);
  }

  export function reveal(r: Rect) {
    const size = sizes[r.page - 1];
    if (!scroller || !size) return;
    // Keep the highlight on the page: line-end glue can report a width past the margin.
    const x = Math.max(0, Math.min(r.x, size.w - 24));
    r = { ...r, x, w: Math.max(24, Math.min(r.w, size.w - x - 36)) };
    const y = tops[r.page - 1] + r.y * scale;
    scroller.scrollTo({ top: y - scroller.clientHeight * 0.33, behavior: "smooth" });
    flash = { ...r, key: Date.now() };
  }

  /** PNG of page one at the current build, for the library shelf. */
  export function hasDoc() {
    return !!doc;
  }

  function onDblClick(e: MouseEvent, i: number) {
    const host = pageEls[i];
    if (!host) return;
    window.getSelection()?.removeAllRanges();
    const r = host.getBoundingClientRect();
    onInverse(i + 1, (e.clientX - r.left) / scale, (e.clientY - r.top) / scale);
  }

  let ro: ResizeObserver;
  onMount(() => {
    ro = new ResizeObserver(() => {
      if (fit) applyFit();
      else renderVisible();
    });
    ro.observe(scroller);
  });
  onDestroy(() => {
    ro?.disconnect();
    clear();
  });

  function toggleNight() {
    night = !night;
    prefs.set("pdf.night", night);
  }
</script>

<div class="pdf">
  <div class="pane-head">
    <span class="title tnum">{sizes.length ? `Page ${pageNum} of ${sizes.length}` : "Preview"}</span>
    <span class="spacer"></span>
    <button class="icon-btn" title="Zoom out ({mod}−)" onclick={() => zoom(-1)} disabled={!sizes.length}><Minus size={16} strokeWidth={1.75} /></button>
    <button class="zoom tnum" title="Fit width ({mod}0)" onclick={() => zoom(0)} disabled={!sizes.length}>
      {sizes.length ? Math.round(scale * 100) + "%" : "–"}
    </button>
    <button class="icon-btn" title="Zoom in ({mod}+)" onclick={() => zoom(1)} disabled={!sizes.length}><Plus size={16} strokeWidth={1.75} /></button>
    <button class="icon-btn" class:on={fit} title="Fit width" onclick={() => setFit(!fit)} disabled={!sizes.length}>
      <MoveHorizontal size={16} strokeWidth={1.75} />
    </button>
    <button class="icon-btn" class:on={night} title="Dim pages" onclick={toggleNight}><Moon size={15} strokeWidth={1.75} /></button>
  </div>

  <div class="scroller" class:night bind:this={scroller} onscroll={onScroll} onwheel={onWheel}>
    {#if sizes.length}
      <div class="column" style="padding:{PAD}px; gap:{GAP}px; min-width:{maxW * scale + PAD * 2}px">
        {#each sizes as s, i (i)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="page"
            bind:this={pageEls[i]}
            style="width:{s.w * scale}px;height:{s.h * scale}px"
            ondblclick={(e) => onDblClick(e, i)}
          >
            {#if flash && flash.page === i + 1}
              {#key flash.key}
                <div
                  class="flash"
                  style="left:{flash.x * scale - 4}px;top:{flash.y * scale - 3}px;width:{flash.w * scale + 8}px;height:{flash.h *
                    scale +
                    6}px"
                ></div>
              {/key}
            {/if}
          </div>
        {/each}
      </div>
    {:else if loadError}
      <div class="empty"><p>Couldn't open the PDF.</p><p class="small">{loadError}</p></div>
    {:else if empty}
      {@render empty()}
    {/if}
  </div>
</div>

<style>
  .pdf {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg-desk);
  }
  .zoom {
    height: 28px;
    min-width: 48px;
    padding: 0 4px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .zoom:hover {
    background: var(--hover);
    color: var(--text);
  }
  .zoom:disabled {
    opacity: 0.35;
  }
  .scroller {
    flex: 1;
    overflow: auto;
    position: relative;
    overscroll-behavior: contain;
  }
  .column {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .page {
    position: relative;
    flex: none;
    background: #fff;
    border-radius: 2px;
    box-shadow: var(--shadow-page);
  }
  .page :global(canvas) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    border-radius: 2px;
  }
  .night .page :global(canvas) {
    filter: invert(0.9) hue-rotate(180deg);
  }
  .night .page {
    background: #1a1a1a;
  }
  .flash {
    position: absolute;
    z-index: 2;
    border-radius: 3px;
    background: rgba(226, 176, 74, 0.28);
    pointer-events: none;
    animation: fade 1.8s ease-out forwards;
  }
  @keyframes fade {
    0%,
    45% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px;
    gap: 4px;
    text-align: center;
    font-size: 12px;
    color: var(--text-2);
  }
  .empty p {
    margin: 0;
  }
  .small {
    color: var(--text-3);
    overflow-wrap: anywhere;
    max-width: 360px;
  }
</style>
