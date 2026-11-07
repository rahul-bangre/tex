# tex — design system

tex is a Mac app for writing LaTeX: a library of papers, an editor, and the typeset PDF.
It should feel like a well-made native Mac tool (Things, Craft, Nova): quiet chrome, the
paper is the hero, every edge lines up. Every screen is built from the rules below; if a
screen needs something not listed here, add it here first.

## Principles

1. **The page is the hero.** Chrome is neutral graphite (dark) or paper white (light). The
   only large areas of colour are rendered PDF pages.
2. **One accent, used sparingly.** Brass (`--accent`) marks the primary action, the
   current selection, focus, and the editor caret. Nothing else is coloured except
   status (ok / warning / error) and syntax.
3. **Legibility over novelty.** Interface text is the system face (SF Pro) at 13px.
   Code is IBM Plex Mono at 13px. Serif appears only for paper titles.
4. **Everything sits on a 4px grid,** and shared horizontal edges line up across panes.
5. **No decoration that isn't information.** No gradients, no glows, no uppercase
   eyebrow labels, no placeholder frames.

## Type

| Role | Face | Size / line | Weight | Tracking |
|---|---|---|---|---|
| Body / controls | `--font-ui` (SF Pro Text) | 13 / 20 | 400, 500 for buttons | -0.08px |
| Secondary / meta | `--font-ui` | 12 / 16 | 400 | 0 |
| Section label | `--font-ui` | 12 / 16 | 600, `--text-2` | 0 (sentence case) |
| Toolbar title | `--font-ui` | 13 / 20 | 600 | -0.08px |
| Paper title (grid) | `--font-serif` (Source Serif 4) | 15 / 20 | 600 | -0.1px |
| Page heading | `--font-serif` | 28 / 34 | 600 | -0.4px |
| Code / editor | `--font-mono` (IBM Plex Mono) | 13 / 21 | 400 | 0 |
| Numbers | any | — | tabular-nums | — |

## Space

Scale: 4, 8, 12, 16, 20, 24, 32, 48. Inner padding of every pane is 12px; page margins 32px.

## Fixed heights (alignment contract)

| Element | Height |
|---|---|
| Window toolbar | 44 (traffic lights centred on 22) |
| Pane header (sidebar, editor, PDF, errors) | 36 — all panes share this so their rules form one line |
| List row (file tree, outline, problems, menus) | 28 |
| Button / input / segmented control | 28 |
| Icon button | 28 × 28, icon 16 |

## Colour tokens

| Token | Dark | Light | Use |
|---|---|---|---|
| `--bg` | #18181A | #FFFFFF | editor, content |
| `--bg-side` | #1F1F22 | #F7F7F8 | sidebar, toolbar, pane headers |
| `--bg-raised` | #2A2A2E | #FFFFFF | menus, popovers, dialogs |
| `--bg-desk` | #111113 | #ECECEE | behind PDF pages |
| `--line` | rgba(255,255,255,.07) | rgba(0,0,0,.08) | hairlines |
| `--line-strong` | rgba(255,255,255,.13) | rgba(0,0,0,.14) | control borders |
| `--text` | #EDEDEF | #1B1B1F | primary |
| `--text-2` | #A0A0A8 | #5E5E66 | secondary |
| `--text-3` | #6B6B73 | #9A9AA2 | tertiary, line numbers |
| `--hover` | rgba(255,255,255,.05) | rgba(0,0,0,.04) | row hover |
| `--selected` | rgba(255,255,255,.09) | rgba(0,0,0,.06) | current row |
| `--accent` | #E2B04A | #A8761A | brass: primary, focus, caret |
| `--accent-ink` | #1A1405 | #FFFFFF | text on accent |
| `--ok` | #5FC48A | #1F8A4C | |
| `--warn` | #E8A13A | #B26A00 | |
| `--err` | #F0645A | #CC3B30 | |

## Shape

* Radius: 6px controls and rows, 10px popovers/dialogs, 2px page thumbnails. Nothing else.
* Borders are 1px hairlines. Shadows only on things that float (menus, dialogs, pages).
* Page shadow: `0 1px 2px rgba(0,0,0,.25), 0 4px 16px rgba(0,0,0,.18)`.

## Components

* **Primary button:** accent fill, `--accent-ink` text, 28 high, 12px side padding, 500 weight.
* **Secondary button:** transparent, `--line-strong` border, `--text`.
* **Icon button:** 28×28, `--text-2` icon, hover `--hover` + `--text`.
* **Status:** 8px dot + 12px text, never a pill.
* **Toggle:** native-looking 26×16 switch, accent when on.
* **Icons:** Lucide, 16px, stroke 1.75.

## Motion

120ms ease-out for hover and colour; 180ms for popovers appearing. No staggered entrances.
