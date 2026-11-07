import { EditorState, Compartment, type Extension } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor,
  highlightSpecialChars,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  StreamLanguage,
  HighlightStyle,
  syntaxHighlighting,
  bracketMatching,
  indentOnInput,
  foldGutter,
  foldKeymap,
} from "@codemirror/language";
import { stex } from "@codemirror/legacy-modes/mode/stex";
import { searchKeymap, highlightSelectionMatches, search } from "@codemirror/search";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
  snippetCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { lintGutter, setDiagnostics, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";

const latex = StreamLanguage.define(stex);

const highlight = HighlightStyle.define([
  { tag: t.tagName, color: "var(--syn-cmd)" },
  { tag: t.atom, color: "var(--syn-env)" },
  { tag: t.keyword, color: "var(--syn-math)" },
  { tag: t.special(t.variableName), color: "var(--syn-math)" },
  { tag: t.number, color: "var(--syn-num)" },
  { tag: t.comment, color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: t.bracket, color: "var(--syn-bracket)" },
  { tag: t.string, color: "var(--syn-env)" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--text)" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    fontSize: "13px",
    lineHeight: "21px",
    overscrollBehavior: "contain",
  },
  ".cm-content": { padding: "12px 0 40vh", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0 24px 0 12px" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--text-3)", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { minWidth: "40px", padding: "0 4px 0 8px", fontSize: "12px" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text-2)" },
  ".cm-activeLine": { backgroundColor: "var(--hover)" },
  ".cm-cursor, .cm-dropCursor": { borderLeft: "2px solid var(--accent)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection) !important",
  },
  ".cm-selectionMatch": { backgroundColor: "var(--selected)" },
  "&.cm-focused .cm-matchingBracket": { backgroundColor: "var(--selected)", outline: "none" },
  ".cm-foldGutter .cm-gutterElement": { color: "var(--text-3)", width: "14px", display: "flex", justifyContent: "center" },
  ".cm-foldPlaceholder": { background: "var(--selected)", border: "none", color: "var(--text-2)", padding: "0 6px", borderRadius: "4px" },
  ".cm-tooltip": {
    background: "var(--bg-raised)",
    border: "none",
    borderRadius: "8px",
    boxShadow: "var(--shadow-pop)",
    color: "var(--text)",
    overflow: "hidden",
  },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--font-mono)", fontSize: "12.5px", maxHeight: "17em", padding: "4px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 8px !important", lineHeight: "20px", borderRadius: "4px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { background: "var(--accent)", color: "var(--accent-ink)" },
  ".cm-completionDetail": { color: "inherit", opacity: 0.55, fontStyle: "normal", marginLeft: "16px", fontFamily: "var(--font-ui)", fontSize: "12px" },
  ".cm-completionMatchedText": { textDecoration: "none", fontWeight: "500" },
  ".cm-completionIcon": { display: "none" },
  ".cm-panels": { background: "var(--bg-side)", color: "var(--text)", borderColor: "var(--line)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-search": { fontFamily: "var(--font-ui)", fontSize: "12px", padding: "8px 12px", display: "flex", flexWrap: "wrap", alignItems: "center", gap: "6px" },
  ".cm-search label": { display: "inline-flex", alignItems: "center", gap: "4px", color: "var(--text-2)" },
  ".cm-search br": { display: "none" },
  ".cm-search input, .cm-search button": { fontFamily: "inherit", fontSize: "12px" },
  ".cm-textfield": {
    height: "24px",
    background: "var(--bg)",
    border: "1px solid var(--line-strong)",
    borderRadius: "6px",
    color: "var(--text)",
    padding: "0 8px",
    margin: "0",
  },
  ".cm-textfield:focus": { borderColor: "var(--accent)" },
  ".cm-button": {
    height: "24px",
    backgroundImage: "none !important",
    background: "transparent",
    border: "1px solid var(--line-strong)",
    borderRadius: "6px",
    color: "var(--text)",
    padding: "0 8px",
    margin: "0",
  },
  ".cm-button:hover": { background: "var(--hover)" },
  ".cm-search [name=close]": { fontSize: "16px", color: "var(--text-3)", padding: "0 6px", right: "8px" },
  ".cm-searchMatch": { backgroundColor: "var(--selected)", outline: "none" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--selection)" },
  ".cm-lintRange-error": { backgroundImage: "none", textDecoration: "underline wavy var(--err)", textUnderlineOffset: "4px", textDecorationThickness: "1px" },
  ".cm-lintRange-warning": { backgroundImage: "none", textDecoration: "underline dotted var(--warn)", textUnderlineOffset: "4px" },
  ".cm-gutter-lint": { width: "12px" },
  ".cm-gutter-lint .cm-gutterElement": { display: "flex", alignItems: "center", justifyContent: "center" },
  ".cm-lint-marker": { width: "6px", height: "6px", content: "none", borderRadius: "50%" },
  ".cm-lint-marker-error": { background: "var(--err)" },
  ".cm-lint-marker-warning": { background: "var(--warn)" },
  ".cm-tooltip-lint": { fontFamily: "var(--font-ui)", fontSize: "12px", padding: "4px" },
  ".cm-diagnostic": { padding: "4px 8px", borderLeft: "none", borderRadius: "4px" },
  ".cm-diagnostic-error": { color: "var(--err)" },
  ".cm-diagnostic-warning": { color: "var(--warn)" },
  ".cm-flash": { animation: "cm-flash 1.4s ease-out" },
});

/* ── Autocompletion ─────────────────────────────────────────── */

const COMMANDS: [string, string?][] = [
  ["section{}", "heading"], ["subsection{}", "heading"], ["subsubsection{}", "heading"], ["paragraph{}", "heading"],
  ["chapter{}", "heading"], ["part{}", "heading"], ["section*{}", "heading"], ["subsection*{}", "heading"],
  ["textbf{}", "bold"], ["textit{}", "italic"], ["emph{}", "emphasis"], ["texttt{}", "mono"], ["textsc{}", "small caps"],
  ["underline{}"], ["textrm{}"], ["textsf{}"], ["mathrm{}"], ["mathbf{}"], ["mathit{}"], ["mathcal{}"], ["mathbb{}"],
  ["mathfrak{}"], ["mathsf{}"], ["boldsymbol{}"], ["operatorname{}"], ["text{}"],
  ["label{}"], ["ref{}"], ["eqref{}"], ["autoref{}"], ["cref{}"], ["Cref{}"], ["pageref{}"],
  ["cite{}"], ["citep{}"], ["citet{}"], ["footnote{}"], ["url{}"], ["href{}{}"],
  ["includegraphics[width=\\linewidth]{}", "figure"], ["caption{}"], ["centering"],
  ["usepackage{}", "preamble"], ["documentclass{}", "preamble"], ["newcommand{}{}"], ["renewcommand{}{}"],
  ["DeclareMathOperator{}{}"], ["title{}"], ["author{}"], ["date{}"], ["maketitle"], ["tableofcontents"],
  ["bibliography{}"], ["bibliographystyle{}"], ["printbibliography"], ["addbibresource{}"], ["appendix"],
  ["input{}"], ["include{}"], ["item"], ["newpage"], ["clearpage"], ["noindent"], ["hline"], ["midrule"],
  ["toprule"], ["bottomrule"], ["vspace{}"], ["hspace{}"], ["quad"], ["qquad"], ["ldots"], ["cdots"], ["dots"],
  ["frac{}{}"], ["dfrac{}{}"], ["sqrt{}"], ["sum"], ["prod"], ["int"], ["oint"], ["lim"], ["infty"], ["partial"],
  ["nabla"], ["cdot"], ["times"], ["leq"], ["geq"], ["neq"], ["approx"], ["equiv"], ["sim"], ["propto"],
  ["in"], ["notin"], ["subset"], ["subseteq"], ["cup"], ["cap"], ["emptyset"], ["forall"], ["exists"],
  ["rightarrow"], ["leftarrow"], ["Rightarrow"], ["Leftrightarrow"], ["mapsto"], ["to"], ["implies"], ["iff"],
  ["alpha"], ["beta"], ["gamma"], ["delta"], ["epsilon"], ["varepsilon"], ["zeta"], ["eta"], ["theta"],
  ["vartheta"], ["iota"], ["kappa"], ["lambda"], ["mu"], ["nu"], ["xi"], ["pi"], ["rho"], ["sigma"], ["tau"],
  ["upsilon"], ["phi"], ["varphi"], ["chi"], ["psi"], ["omega"], ["Gamma"], ["Delta"], ["Theta"], ["Lambda"],
  ["Xi"], ["Pi"], ["Sigma"], ["Phi"], ["Psi"], ["Omega"], ["left("], ["right)"], ["left["], ["right]"],
  ["langle"], ["rangle"], ["lVert"], ["rVert"], ["hat{}"], ["bar{}"], ["tilde{}"], ["vec{}"], ["dot{}"],
  ["overline{}"], ["underbrace{}_{}"], ["overbrace{}^{}"], ["binom{}{}"], ["log"], ["ln"], ["exp"], ["sin"],
  ["cos"], ["tan"], ["max"], ["min"], ["arg"], ["det"], ["Pr"], ["mathbb{E}"], ["mathbb{R}"], ["mathbb{N}"],
  ["mathbb{Z}"], ["textwidth"], ["linewidth"], ["today"], ["LaTeX"], ["TeX"], ["begin{}"], ["end{}"],
];

const ENVIRONMENTS = [
  "document", "abstract", "figure", "figure*", "table", "table*", "tabular", "center", "itemize", "enumerate",
  "description", "equation", "equation*", "align", "align*", "gather", "gather*", "multline", "split", "cases",
  "matrix", "pmatrix", "bmatrix", "vmatrix", "array", "theorem", "lemma", "proposition", "corollary",
  "definition", "remark", "example", "proof", "quote", "quotation", "verbatim", "minipage", "subfigure",
  "thebibliography", "tikzpicture", "lstlisting", "minted", "frame", "columns", "column", "algorithm",
  "algorithmic", "appendices",
];

function commandCompletion(cmd: string, detail?: string): Completion {
  // Turn the first `{}` into a cursor slot, others into further fields.
  let i = 0;
  const tpl = cmd.replace(/\{\}/g, () => (i++ === 0 ? "{${}}" : "{${}}")).replace(/\[([^\]$]*)\]/, "[$1]");
  return snippetCompletion("\\" + tpl, { label: "\\" + cmd.replace(/\{\}/g, "{}"), detail, type: "keyword" });
}

const commandCompletions = COMMANDS.map(([c, d]) => commandCompletion(c, d));

export interface Symbols {
  labels: Set<string>;
  cites: Set<string>;
}

function latexCompletions(getSymbols: () => Symbols) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);

    let m = before.match(/\\(?:[a-zA-Z]*ref|cref|Cref)\{([^}]*)$/);
    if (m) {
      const word = m[1].split(",").pop() ?? "";
      return {
        from: ctx.pos - word.length,
        options: [...getSymbols().labels].map((l) => ({ label: l, type: "variable", detail: "label" })),
        validFor: /^[^},]*$/,
      };
    }
    m = before.match(/\\(?:[a-zA-Z]*cite[a-zA-Z]*)(?:\[[^\]]*\])*\{([^}]*)$/);
    if (m) {
      const word = m[1].split(",").pop()!.trimStart();
      return {
        from: ctx.pos - word.length,
        options: [...getSymbols().cites].map((l) => ({ label: l, type: "variable", detail: "cite" })),
        validFor: /^[^},\s]*$/,
      };
    }
    m = before.match(/\\begin\{([a-zA-Z*]*)$/);
    if (m) {
      const word = m[1];
      const after = line.text.slice(ctx.pos - line.from);
      const closes = after.startsWith("}");
      return {
        from: ctx.pos - word.length,
        options: ENVIRONMENTS.map((env) => ({
          label: env,
          type: "type",
          detail: "environment",
          apply: (view: EditorView, _c: Completion, from: number, to: number) => {
            const indent = /^\s*/.exec(line.text)![0];
            const inner = env === "itemize" || env === "enumerate" ? "\\item " : "";
            const end = closes ? to + 1 : to;
            const text = `${env}}\n${indent}  ${inner}\n${indent}\\end{${env}}`;
            view.dispatch({
              changes: { from, to: end, insert: text },
              selection: { anchor: from + env.length + 2 + indent.length + 2 + inner.length },
            });
          },
        })),
        validFor: /^[a-zA-Z*]*$/,
      };
    }
    m = before.match(/\\([a-zA-Z]*)$/);
    if (m && (m[1].length > 0 || ctx.explicit)) {
      return { from: ctx.pos - m[1].length - 1, options: commandCompletions, validFor: /^\\[a-zA-Z]*$/ };
    }
    return null;
  };
}

/* ── Editor controller ──────────────────────────────────────── */

export interface EditorHooks {
  onChange: (rel: string, text: string) => void;
  onCursor?: (line: number) => void;
  getSymbols: () => Symbols;
  keys: { key: string; run: () => boolean }[];
}

const langCompartment = new Compartment();

function languageFor(rel: string): Extension {
  return /\.(tex|sty|cls|ltx|dtx|bib|bbx|cbx|def)$/i.test(rel) ? latex : [];
}

export class Editor {
  view: EditorView;
  private states = new Map<string, EditorState>();
  private current: string | null = null;
  private baseExtensions: Extension[];

  constructor(parent: HTMLElement, private hooks: EditorHooks) {
    const self = this;
    this.baseExtensions = [
      lineNumbers(),
      foldGutter({ openText: "\u2304", closedText: "\u203a" }),
      lintGutter(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      syntaxHighlighting(highlight),
      bracketMatching(),
      closeBrackets(),
      EditorState.languageData.of(() => [{ closeBrackets: { brackets: ["(", "[", "{"] } }]),
      autocompletion({
        override: [latexCompletions(hooks.getSymbols)],
        icons: false,
        activateOnTypingDelay: 40,
      }),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      search({ top: true }),
      EditorView.lineWrapping,
      keymap.of([
        ...hooks.keys.map((k) => ({ key: k.key, run: k.run, preventDefault: true })),
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      theme,
      EditorView.updateListener.of((u) => {
        if (!self.current) return;
        if (u.docChanged) hooks.onChange(self.current, u.state.doc.toString());
        if (u.selectionSet || u.docChanged) {
          hooks.onCursor?.(u.state.doc.lineAt(u.state.selection.main.head).number);
        }
      }),
      EditorView.contentAttributes.of({ spellcheck: "false", autocorrect: "off", autocapitalize: "off" }),
    ];
    this.view = new EditorView({ parent, state: EditorState.create({ doc: "", extensions: this.baseExtensions }) });
  }

  private makeState(rel: string, text: string) {
    return EditorState.create({
      doc: text,
      extensions: [...this.baseExtensions, langCompartment.of(languageFor(rel))],
    });
  }

  has(rel: string) {
    return this.states.has(rel);
  }

  /** Show `rel`, creating its state from `text` when first opened. */
  open(rel: string, text: string) {
    if (this.current) this.states.set(this.current, this.view.state);
    let st = this.states.get(rel);
    if (!st) {
      st = this.makeState(rel, text);
      this.states.set(rel, st);
    }
    this.current = rel;
    this.view.setState(st);
    requestAnimationFrame(() => this.view.focus());
  }

  get currentFile() {
    return this.current;
  }

  text(rel: string): string | null {
    if (rel === this.current) return this.view.state.doc.toString();
    return this.states.get(rel)?.doc.toString() ?? null;
  }

  /** Replace a document's content from disk, keeping the cursor where sensible. */
  replace(rel: string, text: string) {
    if (rel === this.current) {
      const v = this.view;
      const head = Math.min(v.state.selection.main.head, text.length);
      const old = v.state.doc.toString();
      // Minimal diff: common prefix/suffix.
      let a = 0;
      while (a < old.length && a < text.length && old[a] === text[a]) a++;
      let b = 0;
      while (b < old.length - a && b < text.length - a && old[old.length - 1 - b] === text[text.length - 1 - b]) b++;
      v.dispatch({
        changes: { from: a, to: old.length - b, insert: text.slice(a, text.length - b) },
        selection: { anchor: Math.min(head, text.length) },
      });
    } else {
      this.states.delete(rel);
    }
  }

  forget(rel: string) {
    this.states.delete(rel);
    if (this.current === rel) this.current = null;
  }

  rename(from: string, to: string) {
    const st = this.states.get(from);
    if (st) {
      this.states.delete(from);
      this.states.set(to, st);
    }
    if (this.current === from) this.current = to;
  }

  gotoLine(line: number, flash = true) {
    const doc = this.view.state.doc;
    const l = doc.line(Math.max(1, Math.min(line, doc.lines)));
    this.view.dispatch({
      selection: { anchor: l.from },
      effects: EditorView.scrollIntoView(l.from, { y: "center" }),
    });
    this.view.focus();
    if (flash) {
      requestAnimationFrame(() => {
        const dom = this.view.domAtPos(l.from).node;
        const el = (dom.nodeType === 1 ? dom : dom.parentElement) as HTMLElement | null;
        const lineEl = el?.closest(".cm-line") as HTMLElement | null;
        if (lineEl) {
          lineEl.classList.remove("cm-flash");
          void lineEl.offsetWidth;
          lineEl.classList.add("cm-flash");
          setTimeout(() => lineEl.classList.remove("cm-flash"), 1500);
        }
      });
    }
  }

  cursorLine(): number {
    const s = this.view.state;
    return s.doc.lineAt(s.selection.main.head).number;
  }

  setDiagnostics(items: { line: number; severity: "error" | "warning"; message: string }[]) {
    const doc = this.view.state.doc;
    const diags: CmDiagnostic[] = items
      .filter((d) => d.line >= 1 && d.line <= doc.lines)
      .map((d) => {
        const l = doc.line(d.line);
        const text = l.text;
        const start = l.from + (text.length - text.trimStart().length);
        return { from: start, to: Math.max(start, l.to), severity: d.severity, message: d.message };
      });
    this.view.dispatch(setDiagnostics(this.view.state, diags));
  }

  destroy() {
    this.view.destroy();
  }
}

/** Scan source text for \label{...} keys and .bib entry keys. */
export function scanSymbols(rel: string, text: string, into: { labels: Map<string, Set<string>>; cites: Map<string, Set<string>> }) {
  if (rel.endsWith(".bib")) {
    const keys = new Set<string>();
    for (const m of text.matchAll(/@[a-zA-Z]+\s*\{\s*([^,\s]+)\s*,/g)) {
      if (!/^(string|comment|preamble)$/i.test(m[0].slice(1).split(/[\s{]/)[0])) keys.add(m[1]);
    }
    into.cites.set(rel, keys);
  } else if (rel.endsWith(".tex")) {
    const labels = new Set<string>();
    for (const m of text.matchAll(/\\label\{([^}]+)\}/g)) labels.add(m[1]);
    into.labels.set(rel, labels);
    const items = new Set<string>();
    for (const m of text.matchAll(/\\bibitem(?:\[[^\]]*\])?\{([^}]+)\}/g)) items.add(m[1]);
    into.cites.set(rel, items);
  }
}

export interface OutlineItem {
  level: number;
  title: string;
  line: number;
}

const LEVELS: Record<string, number> = { part: 0, chapter: 1, section: 2, subsection: 3, subsubsection: 4, paragraph: 5 };

export function outline(text: string): OutlineItem[] {
  const out: OutlineItem[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (!l.includes("\\")) continue;
    const m = /^\s*\\(part|chapter|section|subsection|subsubsection|paragraph)\*?\s*(?:\[[^\]]*\])?\s*\{(.*)/.exec(l);
    if (!m) continue;
    // Read the balanced argument (single line is the common case).
    let depth = 1;
    let title = "";
    for (const ch of m[2]) {
      if (ch === "{") depth++;
      if (ch === "}" && --depth === 0) break;
      title += ch;
    }
    title = title
      .replace(/\\(?:label|footnote|thanks)\{[^}]*\}/g, "")
      .replace(/\\[a-zA-Z]+\*?/g, "")
      .replace(/[{}$~]/g, " ")
      .replace(/\s+/g, " ")
      .trim();
    out.push({ level: LEVELS[m[1]], title: title || "Untitled", line: i + 1 });
  }
  return out;
}
