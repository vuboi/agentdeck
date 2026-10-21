---
name: z-ui-design-system
description: Use whenever writing or reviewing Angular UI markup in a project that depends on @shival99/z-ui — before using a raw HTML element (button, input, div-as-card, h1/p) or any z-* component. Ensures the exact right component, size, and design token are used instead of raw HTML or guessed props.
---

# Z-UI Design System

This is the **portable, AI-facing reference** for `@shival99/z-ui` (published npm package, Angular 22 standalone
components, Tailwind v4 + CVA). It plays the role shadcn/ui's docs site plays for a human: it tells an agent
exactly which component to reach for, which size/variant to pick, and which token to style with — instead of
falling back to raw HTML or inventing an `[zFoo]` input that doesn't exist.

Copy this whole `z-ui-design-system/` folder into any consumer project that has `@shival99/z-ui` installed.
It does not depend on this monorepo's internal tooling.

## Hard rules

1. **Never write raw HTML for anything a z-ui component covers.** No `<button>`, `<input>`, `<textarea>`,
   `<select>`, a `<div>` styled to look like a card, etc. Use the matching `z-*` component/directive.
2. **Never invent an input/output/model.** Only use props documented in `reference/*.md` for that component.
   If you're not sure a prop exists, don't guess — say so, or grep the installed package's `.d.ts`
   (`node_modules/@shival99/z-ui/components/<name>/*.d.ts`).
3. **Never hardcode colors.** No `#fff`, no `text-gray-500`, no `bg-slate-100`. Always use the semantic
   Tailwind tokens in `reference/tokens.md` (`bg-primary`, `text-muted-foreground`, `border-input`, ...) — they
   are theme- and dark-mode-aware, raw Tailwind grays are not.
4. **Never set a custom `font-family`.** The app-wide font (`--font-sans`, Outfit by default) is already global.
   Only use Tailwind's `text-*` size classes on top of it — see `reference/typography.md`.
5. **Pick size by context, not by default.** Every sized component (`z-button`, `z-input`, `z-card`, ...) has
   an explicit size scale — match it to the density of the surrounding UI (see each reference doc's size table).
6. **Never add the `block` utility to `z-select`'s `class` input.** Keep the trigger layout managed by the
   component; use a surrounding layout wrapper when external block-level layout is needed.

## Current theming and overlay contract

- Configure the library with `provideZTheme({ defaultTheme, defaultDarkMode, cacheKey })`. The bundled themes
  are `neutral`, `gray`, `slate`, `stone`, `zinc`, `green`, `orange`, `violet`, and `hospital`; see
  [reference/theming.md](reference/theming.md) for the provider and cache behavior.
- Import `assets/css/tailwind.css` once. It bundles the theme files, so changing `ZThemeService.setTheme()` only
  changes the root `z-theme` attribute; it does not load a CSS file at runtime. Third-party CSS (Vidstack for
  `z-media-player`, Univer for the `z-gallery` Excel preview) is **not** bundled there — add
  `assets/css/features/media.css` / `assets/css/features/office.css` only when you use those components, so a
  project that doesn't need them isn't forced to install `vidstack` / `@univerjs/*`. See
  [reference/installation.md](reference/installation.md).
- The base `--radius` is `0.375rem`. Tailwind's `rounded-xs` through `rounded-2xl` derive from it, while
  `rounded-(--radius-pill)` is the theme-aware pill radius. Prefer these tokens over component-specific literals.
- Popover-backed controls (`z-select`, `z-calendar`, `z-autocomplete`, dropdown menus, and command surfaces) use
  the shared placement shadow. Keep the panel's `shadow-[var(--z-popover-shadow)]` and do not add an outer
  `overflow-hidden` that clips the shadow; read [reference/popover.md](reference/popover.md) before customizing it.
- `z-button-group` receives container classes through `zClass`; its direct child group items inherit the theme
  radius through the button variants. Use `[z-button-group-item]` for selectable items and do not manually trim
  their borders/radii.

## Component index

Every component in `@shival99/z-ui` has a reference file. Read the matching one before writing markup for it —
don't rely on memory, and don't guess a prop that isn't in the file.

| Component                     | Selector(s)                                                      | Reference                                                |
| ----------------------------- | ---------------------------------------------------------------- | -------------------------------------------------------- |
| Accordion                     | `z-accordion`                                                    | [reference/accordion.md](reference/accordion.md)         |
| Audio Player                  | `z-audio-player`                                                 | [reference/audio-player.md](reference/audio-player.md)   |
| Autocomplete                  | `z-autocomplete`                                                 | [reference/autocomplete.md](reference/autocomplete.md)   |
| Avatar                        | `z-avatar`                                                       | [reference/avatar.md](reference/avatar.md)               |
| Breadcrumb                    | `z-breadcrumb`                                                   | [reference/breadcrumb.md](reference/breadcrumb.md)       |
| Button                        | `z-button`, `button[z-button]`, `a[z-button]`                    | [reference/button.md](reference/button.md)               |
| Button Group                  | `z-button-group`                                                 | [reference/button-group.md](reference/button-group.md)   |
| Calendar                      | `z-calendar`                                                     | [reference/calendar.md](reference/calendar.md)           |
| Card                          | `z-card`                                                         | [reference/card.md](reference/card.md)                   |
| Chat                          | `z-chat`                                                         | [reference/chat.md](reference/chat.md)                   |
| Checkbox                      | `z-checkbox`                                                     | [reference/checkbox.md](reference/checkbox.md)           |
| Code                          | `z-code`                                                         | [reference/code.md](reference/code.md)                   |
| Number Ticker                 | `z-number-ticker`                                                | [reference/number-ticker.md](reference/number-ticker.md) |
| Command                       | `z-command`                                                      | [reference/command.md](reference/command.md)             |
| Drawer                        | `z-drawer` (+ `ZDrawerService`)                                  | [reference/drawer.md](reference/drawer.md)               |
| Dropdown Menu                 | `z-dropdown-menu`                                                | [reference/dropdown-menu.md](reference/dropdown-menu.md) |
| Editor                        | `z-editor`                                                       | [reference/editor.md](reference/editor.md)               |
| Empty                         | `z-empty`                                                        | [reference/empty.md](reference/empty.md)                 |
| Filter                        | `z-filter`                                                       | [reference/filter.md](reference/filter.md)               |
| Gallery                       | `z-gallery`                                                      | [reference/gallery.md](reference/gallery.md)             |
| Hover Glide                   | `[zHoverGlide]`                                                  | [reference/hover-glide.md](reference/hover-glide.md)     |
| Icon                          | `z-icon`                                                         | [reference/icon.md](reference/icon.md)                   |
| Input                         | `z-input`                                                        | [reference/input.md](reference/input.md)                 |
| Kanban                        | `z-kanban`                                                       | [reference/kanban.md](reference/kanban.md)               |
| Loading                       | `z-loading`                                                      | [reference/loading.md](reference/loading.md)             |
| Marquee                       | `z-marquee`                                                      | [reference/marquee.md](reference/marquee.md)             |
| Media Player                  | `z-media-player`                                                 | [reference/media-player.md](reference/media-player.md)   |
| Menu                          | `z-menu-v2` (primary), `z-menu` (legacy)                         | [reference/menu.md](reference/menu.md)                   |
| Metric Card                   | `z-metric-card`                                                  | [reference/metric-card.md](reference/metric-card.md)     |
| Modal                         | `z-modal` (+ `ZModalService`)                                    | [reference/modal.md](reference/modal.md)                 |
| Overlay (low-level primitive) | `ZOverlayService`                                                | [reference/overlay.md](reference/overlay.md)             |
| Pagination                    | `z-pagination`                                                   | [reference/pagination.md](reference/pagination.md)       |
| Popover                       | `z-popover`, `[z-popover]`                                       | [reference/popover.md](reference/popover.md)             |
| QR Code                       | `z-qrcode`                                                       | [reference/qrcode.md](reference/qrcode.md)               |
| Radio                         | `z-radio`                                                        | [reference/radio.md](reference/radio.md)                 |
| Scroll Area                   | `z-scrollarea`                                                   | [reference/scrollarea.md](reference/scrollarea.md)       |
| Select                        | `z-select`                                                       | [reference/select.md](reference/select.md)               |
| Show More                     | `z-show-more`                                                    | [reference/show-more.md](reference/show-more.md)         |
| Skeleton                      | `z-skeleton`                                                     | [reference/skeleton.md](reference/skeleton.md)           |
| Skeleton Auto                 | `z-skeleton-auto`                                                | [reference/skeleton-auto.md](reference/skeleton-auto.md) |
| Steps                         | `z-steps`                                                        | [reference/steps.md](reference/steps.md)                 |
| Switch                        | `z-switch`                                                       | [reference/switch.md](reference/switch.md)               |
| Table                         | `z-table`                                                        | [reference/table.md](reference/table.md)                 |
| Tabs                          | `z-tabs`                                                         | [reference/tabs.md](reference/tabs.md)                   |
| Tags                          | `z-tags`, `[z-tags]`                                             | [reference/tags.md](reference/tags.md)                   |
| Timeline                      | `z-timeline`                                                     | [reference/timeline.md](reference/timeline.md)           |
| Toast                         | `ZToastService`                                                  | [reference/toast.md](reference/toast.md)                 |
| Tooltip                       | `[z-tooltip]`, `[zTooltip]`                                      | [reference/tooltip.md](reference/tooltip.md)             |
| Upload                        | `z-upload`                                                       | [reference/upload.md](reference/upload.md)               |
| Typography                    | (no dedicated component — Tailwind classes over the global font) | [reference/typography.md](reference/typography.md)       |
| Design tokens                 | CSS custom properties, theme-scoped                              | [reference/tokens.md](reference/tokens.md)               |
| Install / build wiring        | `angular.json` styles, PostCSS, providers, optional deps         | [reference/installation.md](reference/installation.md)   |

### Known gaps / gotchas surfaced while writing these docs

- **`z-audio-player`** requires `zSrc`; it exposes load/error/play events but no autoplay, time-update, or two-way title model.
- **`z-media-player`** switches to a YouTube iframe for YouTube URLs; native-player inputs such as `zControls`, subtitles, and custom player events apply to regular video sources.
- **`z-media-player`**'s `zControls` is a partial visibility map; every control group is visible by default, and `zThumbnailsVtt` expects WebVTT cues with `#xywh=x,y,w,h` sprite references.
- **`z-tabs`**'s `zShowNavWrapper` only adds the elevated navigation surface for glide types (see `Z_TABS_GLIDE_TYPES` / `Z_TABS_OMNI_GLIDE_TYPES` in `z-tabs.define.ts`); use `zClassContent` for classes on the navigation surface/container.
- **`z-button-group`** takes extra classes via `zClass`, not `class` like every other component — easy to get wrong.
- **`z-select`** supports nested options only when `zTree` is enabled; nested options use `children` and drill down
  through a breadcrumb instead of being selectable parent values.
- **`z-select`** selected tags fit content by default; in nowrap mode the auto-width tag whose rendered bounds
  cross the trigger's right clipping edge shrinks and ellipsizes, while `zTagWidth` keeps each tag at a fixed width
  until one crosses the same edge, then that tag shrinks and truncates its text too.
- **`z-select`** custom option templates own their search highlighting; import `ZHighlightPipe` and `ZSafeHtmlPipe`
  from `@shival99/z-ui/pipes`, bind `zOnSearch` to a signal, and compose `option.label | zHighlight: search | zSafeHtml`.
- **`z-loading`** has 36 spinner names, including `orbit`, `bars`, `matrix-grid`, `merge`, `encode`, `scan`, and
  `lift`; read the reference instead of assuming the older 29-style list.
- **`z-popover`** content must remain overflow-visible so shadows from projected dropdown/calendar panels can escape.
- **`z-popover`** closes when its trigger or an ancestor becomes hidden or disconnected during responsive changes; keep
  the trigger visibility state on the directive host or pass the changing anchor through `zTriggerRef`.
- **`z-drawer`**'s footer buttons only render when `zOkText`/`zCancelText` are explicitly set (both default to `null`).
- **`z-pagination`**'s `zSize` input exists but currently has no visual effect (not wired to any styling).
- **`z-qrcode`**'s `zSize` is a raw pixel number, not a `'sm'|'default'|'lg'` token like other components.
- **`z-skeleton`**'s `zSize` only affects the `circle` shape variant; it does nothing for `bar`/`text`.
- **`z-menu`** vs **`z-menu-v2`**: prefer `z-menu-v2` for new work (cleaner expand/collapse, animated submenus); `z-menu` remains for its per-item custom template slot (`menuTemplate`) that v2 lacks.
- **`z-tags`**: the `z-tags` _component_ (tag input/list) and the `[z-tags]` _directive_ (single tag styling) share a selector prefix but are different things — don't conflate them.

## Verifying a prop actually exists

In a consumer project you only have the compiled package, not this monorepo's source. To check a component's
real public API:

```bash
find node_modules/@shival99/z-ui/components/<component-name> -name "*.d.ts"
```

Then read the `.component.d.ts` (or `.directive.d.ts`) file — every `input<T>`, `output<T>`, and `model<T>` is
declared there with its real type. Treat that file as ground truth over memory or assumption.
