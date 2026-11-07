export interface MenuItem {
  label: string;
  hint?: string;
  danger?: boolean;
  disabled?: boolean;
  action: () => void;
}
export type MenuEntry = MenuItem | "sep";

export const ui = $state({
  toasts: [] as { id: number; text: string; tone: "plain" | "error" }[],
  menu: null as null | { x: number; y: number; items: MenuEntry[] },
});

let nextId = 1;

export function toast(text: string, tone: "plain" | "error" = "plain", ms = 3200) {
  const id = nextId++;
  ui.toasts.push({ id, text, tone });
  setTimeout(() => {
    const i = ui.toasts.findIndex((t) => t.id === id);
    if (i >= 0) ui.toasts.splice(i, 1);
  }, ms);
}

export function openMenu(e: MouseEvent | { clientX: number; clientY: number }, items: MenuEntry[]) {
  if ("preventDefault" in e) e.preventDefault();
  ui.menu = { x: e.clientX, y: e.clientY, items };
}

export function closeMenu() {
  ui.menu = null;
}

/** localStorage, but never throws. */
export const prefs = {
  get<T>(key: string, fallback: T): T {
    try {
      const v = localStorage.getItem("tex:" + key);
      return v == null ? fallback : (JSON.parse(v) as T);
    } catch {
      return fallback;
    }
  },
  set(key: string, value: unknown) {
    try {
      localStorage.setItem("tex:" + key, JSON.stringify(value));
    } catch {
      /* ignore */
    }
  },
};

export function relTime(ms: number): string {
  if (!ms) return "never";
  const s = (Date.now() - ms) / 1000;
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  if (s < 86400 * 7) return `${Math.round(s / 86400)}d ago`;
  const d = new Date(ms);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", ...(sameYear ? {} : { year: "numeric" }) });
}

export const isMac = navigator.platform.toLowerCase().includes("mac");
export const mod = isMac ? "⌘" : "Ctrl+";
