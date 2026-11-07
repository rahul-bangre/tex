import { invoke } from "@tauri-apps/api/core";

export interface ProjectInfo {
  name: string;
  path: string;
  main: string | null;
  title: string | null;
  modified: number;
  opened: number;
  pdf: string | null;
  pdfMtime: number;
  thumb: string | null;
  thumbMtime: number;
  external: boolean;
  folder: string;
}

export interface Entry {
  path: string;
  dir: boolean;
  size: number;
}

export interface Diagnostic {
  severity: "error" | "warning" | "box";
  file: string | null;
  line: number | null;
  message: string;
}

export interface CompileResult {
  ok: boolean;
  pdf: string | null;
  ms: number;
  needsRerun: boolean;
  diagnostics: Diagnostic[];
  output: string;
}

export interface Rect {
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export const api = {
  appInfo: () => invoke<{ root: string; tectonic: string | null }>("app_info"),
  setRoot: (path: string) => invoke<void>("set_root", { path }),
  watchLibrary: () => invoke<void>("watch_library"),
  listLibrary: () => invoke<{ projects: ProjectInfo[]; folders: string[] }>("list_library"),
  createFolder: (parent: string, name: string) => invoke<string>("create_folder", { parent, name }),
  renameFolder: (folder: string, name: string) => invoke<string>("rename_folder", { folder, name }),
  trashFolder: (folder: string) => invoke<void>("trash_folder", { folder }),
  moveProject: (path: string, folder: string) => invoke<ProjectInfo>("move_project", { path, folder }),
  clipboardText: () => invoke<string>("clipboard_text"),
  createProject: (name: string, template: string, folder = "") => invoke<ProjectInfo>("create_project", { name, template, folder }),
  importZip: (path: string, folder = "") => invoke<ProjectInfo>("import_zip", { path, folder }),
  addFolder: (path: string) => invoke<ProjectInfo>("add_folder", { path }),
  forgetProject: (path: string) => invoke<void>("forget_project", { path }),
  trashProject: (path: string) => invoke<void>("trash_project", { path }),
  openProject: (path: string) => invoke<{ info: ProjectInfo; tree: Entry[] }>("open_project", { path }),
  closeProject: () => invoke<void>("close_project"),
  listTree: (project: string) => invoke<Entry[]>("list_tree", { project }),
  readText: (project: string, rel: string) => invoke<string>("read_text", { project, rel }),
  writeText: (project: string, rel: string, content: string) => invoke<number>("write_text", { project, rel, content }),
  readBytes: (project: string, rel: string) => invoke<ArrayBuffer>("read_bytes", { project, rel }),
  createEntry: (project: string, rel: string, dir: boolean) => invoke<void>("create_entry", { project, rel, dir }),
  renameEntry: (project: string, from: string, to: string) => invoke<void>("rename_entry", { project, from, to }),
  trashEntry: (project: string, rel: string) => invoke<void>("trash_entry", { project, rel }),
  importFiles: (project: string, dir: string, paths: string[]) => invoke<string[]>("import_files", { project, dir, paths }),
  setMain: (project: string, rel: string) => invoke<void>("set_main", { project, rel }),
  compile: (project: string, main: string, full = false) => invoke<CompileResult>("compile", { project, main, full }),
  readPdf: (path: string) => invoke<ArrayBuffer>("read_pdf", { path }),
  synctexForward: (project: string, main: string, rel: string, line: number) =>
    invoke<Rect | null>("synctex_forward", { project, main, rel, line }),
  synctexInverse: (project: string, main: string, page: number, x: number, y: number) =>
    invoke<{ rel: string; line: number } | null>("synctex_inverse", { project, main, page, x, y }),
  exportPdf: (src: string, dest: string) => invoke<void>("export_pdf", { src, dest }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  openExternal: (path: string) => invoke<void>("open_external", { path }),
  saveThumb: (project: string, png: Uint8Array) =>
    invoke<void>("save_thumb", png, { headers: { "x-project": encodeURIComponent(project) } }),
  readThumb: (project: string) => invoke<ArrayBuffer>("read_thumb", { project }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as Error).message);
  return String(e);
}
