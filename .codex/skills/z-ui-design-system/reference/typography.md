# Typography

Z-UI has **no dedicated typography component** (no `z-heading`, no `z-text`). Text is plain HTML
(`h1`–`h6`, `p`, `span`) styled with Tailwind's `text-*`/`font-*` utilities on top of the global font — this is
a real gap compared to shadcn/ui's `<Typography>` primitives, so the rule here is about **consistent
convention**, not a component API.

## Font

- Body font is set once, globally, on `body` — `var(--font-sans)` = `'Outfit', system-ui, -apple-system, sans-serif` by default.
  **Never** set `font-family` yourself; every element inherits it already.
- A monospace token exists — `var(--font-mono)` — use `font-mono` only for code, numeric/tabular data, or
  IDs, not for general UI text.
- Never load or reference a different font (no Google Fonts `<link>`, no `font-['Inter']`, etc.) — it would
  visually diverge from every z-ui component, which is built against the library's configured `--font-sans` metrics.

## Recommended text scale (convention, cross-checked against component internals)

This mirrors the sizes z-ui's own components already use internally (`z-card` title: `sm`→`text-sm`,
`default`→`text-base`, `lg`→`text-lg`; `z-button`/`z-input` body text: `text-sm`; `z-button` `xl`: `text-base`).
Use it so free-standing text matches component text at the same "density level":

| Role                       | Classes                                           | Use for                                                                  |
| -------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------ |
| Page title (h1)            | `text-2xl font-semibold text-foreground`          | Top-level page heading                                                   |
| Section title (h2)         | `text-xl font-semibold text-foreground`           | Major section heading                                                    |
| Card/subsection title (h3) | `text-base font-medium text-foreground`           | Matches `z-card`'s default title size                                    |
| Body                       | `text-sm text-foreground`                         | Default paragraph/label text — matches `z-input`/`z-button` default size |
| Small/dense body           | `text-xs text-foreground`                         | Matches `sm`-sized components (table cells, compact rows)                |
| Helper/caption text        | `text-xs text-muted-foreground`                   | Field hints, timestamps, secondary metadata                              |
| Muted body                 | `text-sm text-muted-foreground`                   | De-emphasized paragraph text                                             |
| Destructive/error text     | `text-sm text-destructive`                        | Validation errors, warnings (matches `z-input`'s own error styling)      |
| Link                       | `text-primary underline-offset-4 hover:underline` | Inline text link (same classes `z-button`'s `zType="link"` uses)         |

Weight: default body text needs no explicit `font-*` (browser default 400 reads correctly against Outfit). Use
`font-medium` for emphasis/labels and `font-semibold` for headings — avoid `font-bold` unless a
design explicitly calls for it, to stay consistent with the weights above.

## Anti-patterns

```html
<!-- WRONG: raw gray, ignores dark mode -->
<p class="text-sm text-gray-500">Optional field</p>
<!-- RIGHT -->
<p class="text-muted-foreground text-sm">Optional field</p>

<!-- WRONG: ad-hoc heading size picked by eye -->
<h2 class="text-[22px] font-bold">Section</h2>
<!-- RIGHT -->
<h2 class="text-foreground text-xl font-semibold">Section</h2>

<!-- WRONG: custom font import -->
<link href="https://fonts.googleapis.com/css2?family=Inter" rel="stylesheet" />
```
