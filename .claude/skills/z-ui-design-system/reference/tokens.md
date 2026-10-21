# Design tokens

All colors ship as CSS custom properties, scoped per theme via `:root[z-theme='<name>']`, with a `.dark`
variant for dark mode. Themes available: `neutral` (the service fallback), `gray`, `slate`, `stone`, `zinc`,
`green`, `orange`, `violet`, and `hospital`. Every bundled theme starts with `--radius: 0.375rem`.

**Rule: always reference the semantic token name (via its Tailwind utility), never a raw hex or a generic
Tailwind gray/slate/zinc scale class.** Raw Tailwind colors don't switch with the theme or dark mode; these do.

## Color tokens → Tailwind utility

| Token                                        | Tailwind utility                                 | Meaning                               |
| -------------------------------------------- | ------------------------------------------------ | ------------------------------------- |
| `--background` / `--foreground`              | `bg-background` / `text-foreground`              | Page background / default text        |
| `--card` / `--card-foreground`               | `bg-card` / `text-card-foreground`               | Card surface                          |
| `--popover` / `--popover-foreground`         | `bg-popover` / `text-popover-foreground`         | Popover/dropdown surface              |
| `--primary` / `--primary-foreground`         | `bg-primary` / `text-primary-foreground`         | Primary brand action                  |
| `--secondary` / `--secondary-foreground`     | `bg-secondary` / `text-secondary-foreground`     | Secondary action                      |
| `--muted` / `--muted-foreground`             | `bg-muted` / `text-muted-foreground`             | Low-emphasis background / helper text |
| `--accent` / `--accent-foreground`           | `bg-accent` / `text-accent-foreground`           | Hover/active surface                  |
| `--destructive` / `--destructive-foreground` | `bg-destructive` / `text-destructive-foreground` | Destructive action                    |
| `--border`                                   | `border-border` (or bare `border`)               | Default border                        |
| `--input`                                    | `border-input`                                   | Form control border                   |
| `--ring`                                     | `ring-ring`                                      | Focus ring                            |
| `--sidebar*`                                 | `bg-sidebar`, etc.                               | Sidebar surface family                |
| `--chart-1..5`                               | `bg-chart-1` etc.                                | Chart series colors                   |

Status colors used by components like `z-button` (`success`, `info`, `warning`, `error`) are **not** theme
tokens — they're fixed Tailwind palette colors (`green-600`, `blue-600`, `amber-500`, `red-600`) with dark-mode
variants baked into each component's own CVA variants. Use the component's own `zType`/`zStatus` variant for
these rather than applying the color utility yourself.

## Radius, shadows, and fonts

`--radius` is the single corner-radius control. Tailwind v4 derives the component scale from it at build time:

| Token           | Formula                               | Typical utility           |
| --------------- | ------------------------------------- | ------------------------- |
| `--radius-xs`   | `max(0px, calc(var(--radius) - 6px))` | `rounded-xs`              |
| `--radius-sm`   | `max(0px, calc(var(--radius) - 4px))` | `rounded-sm`              |
| `--radius-md`   | `max(0px, calc(var(--radius) - 2px))` | `rounded-md`              |
| `--radius-lg`   | `var(--radius)`                       | `rounded-lg`              |
| `--radius-xl`   | `calc(var(--radius) * 1.2)`           | `rounded-xl`              |
| `--radius-2xl`  | `calc(var(--radius) * 1.4)`           | `rounded-2xl`             |
| `--radius-pill` | `calc(infinity * 1px)`                | `rounded-(--radius-pill)` |

The default `--radius` is `0.375rem` (`6px`), including the base fallback and bundled themes. A custom value is
safe at `0`: the derived finite radii clamp to `0`, while `--radius-pill` remains a true pill token. Use
`rounded-full` only for shapes that should stay circular regardless of the theme radius.

| Token                | Default value                                                    | Utility             |
| -------------------- | ---------------------------------------------------------------- | ------------------- |
| `--shadow-0`         | `0 4px 8px 0 #02020312`                                          | `.shadow-0`         |
| `--shadow-1`         | `0 4px 8px 0 #02020329`                                          | `.shadow-1`         |
| `--shadow-2`         | `0 8px 16px 0 #02020352`                                         | `.shadow-2`         |
| `--shadow-3`         | `0 16px 32px 0 #0202037a`                                        | `.shadow-3`         |
| `--shadow-card`      | `0 0 5px 0 rgba(0,0,0,.1), 0 0 1px 0 rgba(0,0,0,.1)`             | `.shadow-card`      |
| `--shadow-card-md`   | `0 2px 8px rgba(0,0,0,.15)`                                      | `.shadow-card-md`   |
| `--shadow-card-lg`   | `0 4px 8px -2px rgba(9,30,66,.25), 0 0 0 1px rgba(9,30,66,.08)`  | `.shadow-card-lg`   |
| `--shadow-card-soft` | `0 0 .25em rgba(67,71,85,.27), 0 .25em 1em rgba(90,125,188,.05)` | `.shadow-card-soft` |

`z-popover` uses a placement-aware `--z-popover-shadow` on the overlay host. Dropdown/calendar panels that use a
`zClass` such as `shadow-none` on the trigger keep their own `shadow-[var(--z-popover-shadow)]`; do not replace it
with a generic shadow or clip it with `overflow-hidden`.

| Token         | Default value                                                                                 | Usage                                       |
| ------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `--font-sans` | `'Outfit', system-ui, -apple-system, sans-serif`                                              | applied globally on `body`, do not override |
| `--font-mono` | `ui-monospace, 'Cascadia Code', 'Source Code Pro', Menlo, Consolas, 'Courier New', monospace` | code/numeric-tabular display                |

## Anti-patterns

```html
<!-- WRONG: raw color, ignores theme/dark mode -->
<div class="border-gray-300 bg-gray-100 text-gray-800">...</div>

<!-- RIGHT -->
<div class="bg-muted text-foreground border-border">...</div>
```

```html
<!-- WRONG: custom font -->
<p style="font-family: Inter, sans-serif">...</p>

<!-- RIGHT: inherit the global font-sans, only set size/weight -->
<p class="text-sm">...</p>
```
