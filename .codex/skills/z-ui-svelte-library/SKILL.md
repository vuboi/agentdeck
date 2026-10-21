---
name: z-ui-svelte-library
description: Use when writing Svelte 5 UI in an app that depends on @shival99/z-ui-svelte, before using raw HTML controls or any Z* component. Covers setup facts, the z* prop conventions, theming, icons, i18n and where to read each component's exact props.
---

# @shival99/z-ui-svelte (consumer guide)

Svelte 5 (runes) component library with the same `z*` API as the Angular `@shival99/z-ui`. Use its components
instead of raw `<button>`, `<input>`, cards, tabs, tables, modals. Do not guess props: read them (see below).

## Setup facts

- Installed and wired by `npx @shival99/z-ui-init init` (writes `z-ui.json` with `"framework": "svelte"`).
  Check health with `npx @shival99/z-ui-init doctor`; optional-peer components via `npx @shival99/z-ui-init add <name>`.
- The app stylesheet must contain, in order: `@import 'tailwindcss';` then `@import '@shival99/z-ui-svelte/styles.css';`
  and be imported once from the app entry (`src/main.ts` or `src/routes/+layout.svelte`). Tailwind v4 runs through
  `@tailwindcss/vite` (or `@tailwindcss/postcss`).
- Import everything from the package root: `import { ZButton, ZSwitch } from '@shival99/z-ui-svelte';`
- Optional peers: `z-skeleton-auto` needs `boneyard-js`; `z-media-player` needs `vidstack`, `hls.js`, `dashjs`;
  `z-gallery` also needs `docx-preview`, `@aiden0z/pptx-renderer`.

## Where to read exact props

Never invent a prop. Each component's API is the `interface Props` at the top of its `.svelte` file:

```
node_modules/@shival99/z-ui-svelte/dist/components/z-<name>/Z<Name>.svelte
node_modules/@shival99/z-ui-svelte/dist/components/z-<name>/z-<name>.types.d.ts   (sizes, variants, option types)
```

Also check `index.d.ts` there for the exported types and variants. In this monorepo the same files live under
`libs/svelte-ui/src/lib/components/`.

## Conventions

- Inputs are `z`-prefixed (`zSize`, `zVariant`, `zDisabled`, `zLoading`, `zLabel`); `class` and `id` are plain.
- Two-way values use `bind:`: `<ZSwitch bind:checked={enabled} />`. Other components expose their own bindable
  name; confirm it in `Props` (`$bindable`).
- Events are callback props, not DOM events: `onChange`, `onFocus`, `onBlur`, and component-specific `on*`.
- Extra styling hooks: `class` on the host plus `z<Part>Class` (for example `zSwitchClass`, `zThumbClass`).
- Content is composed with snippets (`{#snippet name()}` / `{@render}`) where a component exposes them, not slots.
- Some components return an imperative API through `bind:this` (for example `focus()` / `blur()`); check `Props`
  and the script exports.
- Svelte 5 only: use `$state`, `$derived`, `onclick={...}`; never `on:click` or legacy stores in new code.

## Theme, dark mode, language, icons

- Theme: set the `z-theme` attribute on `<html>` (`neutral`, `gray`, `slate`, `stone`, `zinc`, `green`, `orange`,
  `violet`, `hospital`). `npx @shival99/z-ui-init theme <name>` changes it. All themes ship in `styles.css`.
- Dark mode: toggle the `dark` class on `<html>`.
- Language: `zSetLanguage('vi' | 'en')`; library text goes through `zTranslate(key, params?)`. Add app strings with
  `zRegisterTranslations(lang, { key: 'text' })` or plug an external library via `zSetTranslator`.
- Icons: `<ZIcon zType="lucideCheck" />`. Icons not bundled load from a CDN by default; to bundle specific ones
  offline call `zProvideIcons({ lucideCheck })` once at startup with `@ng-icons/lucide` exports.

## Common mistakes

- Missing `styles.css` import or Tailwind plugin: components render unstyled. Run `doctor`.
- Using an optional-peer component without its packages: run `npx @shival99/z-ui-init add <name>`.
- Hardcoding colors: use theme tokens (`bg-primary`, `text-muted-foreground`, `border-input`).
- Rendering user HTML with `{@html}` without sanitizing it first.
