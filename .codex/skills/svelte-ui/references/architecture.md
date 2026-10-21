# Svelte UI Architecture Reference

Facts about the Svelte 5 side of Z-UI. Verify against source before relying on a detail.

## Workspace map

| Path                                                                              | Role                                                                                             |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `libs/svelte-ui`                                                                  | Package `@shival99/z-ui-svelte` (Svelte `^5.40` peer, runes, no CDK). Built by `svelte-package`. |
| `libs/svelte-ui/src/lib/index.ts`                                                 | Single public entry. Every component is re-exported as `export * from './components/z-xxx'`.     |
| `libs/svelte-ui/src/lib/components`                                               | One folder per component: `z-<name>/`.                                                           |
| `libs/svelte-ui/src/lib/{utils,services,i18n,motion,overlay,interactions,styles}` | Shared runtime. Reuse before adding anything.                                                    |
| `apps/z-ui-svelte`                                                                | Vite demo/docs app (`@z-ui/svelte-demo`). Imports the lib as `@shival99/z-ui-svelte`.            |

The Svelte lib mirrors the Angular `libs/core-ui` public contract (`z*` prop names, variants, sizes). Keep names
aligned, but write idiomatic Svelte 5, not a mechanical Angular port. There is no Angular `inject`, `signal`, or
`input()` here.

## Component folder blueprint

```
z-<name>/
  Z<Name>.svelte          component (PascalCase file)
  z-<name>.types.ts       public/reused types only (no runtime)
  z-<name>.define.ts      constants/config/enums only
  z-<name>.variants.ts    CVA variants only
  z-<name>.utils.ts       pure functions + consts only (no types)
  z-<name>.component.spec.ts   Vitest mount tests
  z-<name>.utils.spec.ts       pure logic tests
  index.ts                local barrel
```

Reference implementation: `components/z-switch` (small) and `components/z-table` (large, with
`*.state.svelte.ts` / `*.reactive.svelte.ts` classes holding reactive state outside the `.svelte` file).

`index.ts` pattern:

```ts
export { default as ZSwitch } from './ZSwitch.svelte';
export type { ZSwitchSize } from './z-switch.types';
export { zSwitchVariants } from './z-switch.variants';
```

File-content rules are enforced by `node scripts/z-architecture-check.mjs <files>`: no top-level `interface`/`type`/
`enum`/function/variable in implementation files. A `<script>` block local `interface Props` and small local
`const`s inside a `.svelte` file are the accepted exception already used across the lib; shared types still go in
`*.types.ts`.

## Component authoring pattern

Follow `ZSwitch.svelte`:

- `interface Props` + `let { ... }: Props = $props();` with defaults in the destructure.
- Public inputs are `z`-prefixed (`zSize`, `zVariant`, `zDisabled`, `zLoading`, `zLabel`). Plain DOM-ish ones stay
  unprefixed: `class`, `id`.
- Two-way value: `checked = $bindable(false)` and consumer uses `bind:checked`. The Angular `zChecked` model maps to
  this. Callbacks are `onChange`, `onFocus`, `onBlur` (not `zChange` outputs).
- Extra class hooks: `class` (host) plus `z<Part>Class` (e.g. `zSwitchClass`, `zThumbClass`), merged with
  `zMergeClasses(variants({...}), custom)` from `utils/merge-classes`.
- Derived classes/state in `$derived`; DOM refs via `$state<HTMLElement | null>(null)` + `bind:this`.
- Unique ids: `$props.id()` (`z-switch-${uid}`).
- Imperative API for `bind:this`: `export function focus()` inside the component script.
- Event attributes (`onclick`, `onanimationend`); never `on:click`. Composition via snippets (`{#snippet}` /
  `{@render}`), not slots, in new code.
- Icons: `<ZIcon zType="lucideX" />`. Library-internal icons are bundled; any other name is fetched from a CDN
  and cached unless the app bundles it up front with `zProvideIcons({...})` (see `apps/z-ui-svelte/src/main.ts`).
  Tune loading with `zConfigureIcons`.
- Text: user-facing strings go through `zTranslate(key, params?)` from `i18n/translate.svelte`. Built-in keys live in
  the `BUILT_IN` `en` and `vi` dictionaries of `libs/svelte-ui/src/lib/i18n/translate.svelte.ts`, named
  `i18n_z_ui_<feature>_<name>` with `{{param}}` placeholders. Declare the identical key in both `en` and `vi` before
  using it. Unknown keys return the raw key/text. Apps extend via `zRegisterTranslations`, `zSetLanguage`,
  `zSetTranslator`.
- Styling: Tailwind v4 + CVA, theme tokens (`bg-primary`, `bg-input`, `--radius-pill`, `--color-primary`), no
  hardcoded colors. Shared animations/utilities live in `styles/` (e.g. `z-animate-wave`).
- `{@html}` only with sanitized/trusted input (see `utils/z-safe-html.ts`, `isomorphic-dompurify`).

## Reactive state outside components

- Files that use runes outside a component must be named `*.svelte.ts` (e.g. `z-persisted.svelte.ts`,
  `z-table.state.svelte.ts`, `z-text-stream.svelte.ts`).
- Prefer a class with `#private = $state()` fields and getters/setters (see `ZPersisted`: `.current` read/write,
  backed by `ZCacheService`, syncs across tabs). Never export a reassigned `$state` primitive from a module; export
  an object/class instead.
- Do not use `$effect` to derive state; use `$derived`. `$effect` is for DOM/browser/library side effects with cleanup.

## Shared runtime to reuse first

- `utils/`: `merge-classes` (`zMergeClasses`), `browser`, `refs`, `text`, `format-num`, `validators`, `math`,
  `async-options`, `z-safe-html`, `z-text-split`, `z-text-stream.svelte`.
- `services/`: `ZCacheService`, `ZPersisted`, `ZHttpAbstractService`, `ZIndexDbService`, `ZExcelService`.
- `overlay/`: `ZPortal`, `ZOverlayOutlet`, `zOverlayStack` for modal/drawer/popover/tooltip layering.
  `@floating-ui/dom` handles positioning.
- `interactions/`, `motion/`: shared gesture and animation helpers.
- Table: `@tanstack/svelte-table`, `@tanstack/svelte-virtual`; toasts: `svelte-sonner`.

Search these (`rg`) before creating a helper; AGENTS.md bans trivial normalize/convert wrappers.

## Demo app contract (`apps/z-ui-svelte`)

A new/changed public component needs four demo touches:

1. `src/examples/<name>/<scenario>.svelte`: one small file per scenario, importing from
   `@shival99/z-ui-svelte`. They are auto-discovered by `src/examples/registry.ts` via `import.meta.glob`, and
   referenced by id `'<name>/<scenario>'`.
2. `src/pages/<name>/<Name>Doc.svelte`: builds `sections: DocExampleSection[]` (`id`, `title`, `description`,
   `example`) and renders `<DocPage {sections} api={[{ id: 'props', title: 'Props', rows: X_PROP_ROWS }]} />`
   from `../../shared/DocPage.svelte`.
3. `src/pages/api-rows.ts`: export `X_PROP_ROWS: DocApiRow[]` (`name`, `type`, `default?`, `description`). Keep it
   in sync with real props, including `bind:` entries.
4. `src/lib/catalog.ts`: add an item to `DOCS_NAVIGATION` (`id`, `label`, `icon`, `description`,
   `load: () => import('../pages/<name>/<Name>Doc.svelte')`). The nav `icon` is a `ZIconName`; unbundled
   icons load from the CDN.

Demo text in this app is plain English in the docs; do not put demo-only copy into the library.

## Testing pattern

Vitest 4 + jsdom, specs next to source. Use the real Svelte API:

```ts
import { flushSync, mount, unmount } from 'svelte';
component = mount(ZSwitch, { target, props });
flushSync();
// interact via DOM (button.click()), flushSync(), assert attributes/classes/callbacks
afterEach(() => {
  unmount(component);
  target.remove();
});
```

Cover default and edge inputs, keyboard/pointer, disabled/loading/readonly, state transitions, cleanup. Test pure
logic directly in `*.utils.spec.ts`. Never weaken a test to pass.

## Commands

```bash
pnpm start:svelte                                  # demo app (scripts/z-start.mjs svelte)
pnpm --filter @shival99/z-ui-svelte check          # svelte-check (lib)
pnpm --filter @shival99/z-ui-svelte test           # vitest run (lib)
pnpm nx run z-ui-svelte:check                      # svelte-check (demo app)
pnpm test:svelte-package                           # packaging smoke test
node scripts/z-architecture-check.mjs <files...>   # placement gate
node scripts/z-lint-changed.mjs <files...>         # lint only task files
pnpm skills:sync && pnpm skills:check              # regenerate skill entrypoints after editing .codex/skills
```

Lint/format only the files touched by the task (never `--git`/repo-wide unless asked).

## New component checklist

1. Search `libs/svelte-ui` and Angular `libs/core-ui` for an equivalent; mirror its `z*` API.
2. Create the folder blueprint above; reuse `ZIcon`, `zMergeClasses`, overlay and motion helpers.
3. Export from the local `index.ts` and add `export * from './components/z-<name>'` to `lib/index.ts`.
4. i18n keys in both locales; no hardcoded colors; keyboard/focus/disabled/reduced-motion handled.
5. Demo: examples, `<Name>Doc.svelte`, API rows, catalog entry, icon registration.
6. Specs, then run check, test, architecture check, and changed-file lint.

## Common pitfalls

- Mutating a `$props` value that is not `$bindable` (use local `$state` seeded from the prop, or make it bindable).
- Putting runes in a plain `.ts` file (must be `.svelte.ts`).
- Using `$effect` to sync derived state, or forgetting its cleanup return.
- Adding types/interfaces to `*.utils.ts` or runtime code to `*.types.ts` (architecture check fails).
- Forgetting the root `lib/index.ts` export or the demo `catalog.ts` entry, so the component is invisible.
- Rendering unsanitized HTML through `{@html}`.

## Packaging and the install CLI

- `pnpm --dir libs/svelte-ui package` runs `svelte-package` then `scripts/bundle-core-styles.mjs`, which copies the
  shared base/theme CSS from `libs/core-ui/assets/css` into `dist/styles/core` and rewrites the imports in
  `dist/styles/z-ui.css`. Without it the published stylesheet imports a path outside the package. Do not add imports
  that escape `libs/svelte-ui` to any CSS under `src/lib/styles`.
- `pnpm test:svelte-package` packs, asserts the required files (including `dist/styles/core/*`) and builds the demo.
- `npx @shival99/z-ui-init` (`libs/z-ui-init`) is the all-in-one installer for both frameworks. It auto-detects a
  Svelte app (`svelte` dependency, no `@angular/core`) or takes `--framework svelte`. Svelte logic lives in
  `libs/z-ui-init/lib/svelte.mjs`; optional-peer components are declared in `SVELTE_FEATURES`
  (`lib/contracts.mjs`). When a Svelte component gains a new optional peer, update `SVELTE_FEATURES`, the package
  `peerDependenciesMeta`, and the consumer skill `.codex/skills/z-ui-svelte-library`, then run `pnpm sync-skill`.
