import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

let port: Worker | null = null;

function ensureWorker() {
  if (!port) {
    port = new Worker(workerUrl, { type: "module" });
    pdfjs.GlobalWorkerOptions.workerPort = port;
  }
}

export type PDFDoc = pdfjs.PDFDocumentProxy;
export type PDFPage = pdfjs.PDFPageProxy;

export async function loadPdf(data: ArrayBuffer): Promise<PDFDoc> {
  ensureWorker();
  const task = pdfjs.getDocument({
    data: new Uint8Array(data),
    enableXfa: false,
    disableAutoFetch: true,
    stopAtErrors: false,
  });
  return task.promise;
}

/** Render page one to a PNG for the library shelf. */
export async function renderThumb(data: ArrayBuffer, width = 680): Promise<{ png: Uint8Array; pages: number }> {
  const doc = await loadPdf(data);
  try {
    const page = await doc.getPage(1);
    const base = page.getViewport({ scale: 1 });
    const viewport = page.getViewport({ scale: width / base.width });
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(viewport.width);
    canvas.height = Math.round(viewport.height);
    // "print" intent paces rendering with timers rather than requestAnimationFrame, which
    // macOS pauses for background windows; thumbnails must render while tex is behind.
    await page.render({ canvas, viewport, intent: "print" }).promise;
    const blob: Blob = await new Promise((res, rej) =>
      canvas.toBlob((b) => (b ? res(b) : rej(new Error("thumbnail encode failed"))), "image/png"),
    );
    return { png: new Uint8Array(await blob.arrayBuffer()), pages: doc.numPages };
  } finally {
    destroyDoc(doc);
  }
}

export function destroyDoc(doc: PDFDoc | null | undefined) {
  if (doc) doc.loadingTask.destroy().catch(() => {});
}
