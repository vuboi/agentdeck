---
name: svelte-ui
description: Use when building, reviewing, testing, or documenting Svelte 5 components in libs/svelte-ui or apps/z-ui-svelte.
---

# Svelte UI

Use this skill for Svelte 5 work in Z-UI. Keep the implementation compatible with the
existing Angular library contract, but do not port Angular code mechanically.

## Before editing

1. Read the repository `AGENTS.md` and inspect one or two nearby Svelte components.
2. Search exact symbols and public exports with `rg`; use CodeGraph or Serena when the
   change affects shared behavior, callers, or a public component contract.
3. Reuse existing Z-UI primitives, CVA variants, `zMergeClasses`, browser helpers, and
   motion utilities before adding a new abstraction.
4. For a new public component, identify its local `index.ts`, root Svelte export, demo
   page, API rows, and focused tests before implementation.
5. Read [architecture reference](references/architecture.md) for the folder blueprint,
   component authoring pattern, i18n, demo-app registration, testing, and pitfalls. Load it
   for any new component, cross-file change, or unfamiliar area; skip it for a one-line fix.

## Svelte 5 conventions

- Use runes: `$props`, `$state`, `$derived`, `$effect`, and `$bindable`.
- Prefer event attributes such as `onclick` and `onkeydown`; do not introduce legacy
  `on:click` syntax in new code.
- Keep derived values in `$derived`; reserve `$effect` for external side effects such
  as DOM, browser, or imperative library integration.
- Keep public props typed and destructured from `$props`; preserve existing `z*` naming,
  bindable values, and callback contracts.
- Keep expensive work out of templates. Put reusable pure behavior in `*.utils.ts`,
  component-family types in `*.types.ts`, static data in `*.define.ts`, and CVA values
  in `*.variants.ts`.
- Utility files may contain functions and constants only. Do not put interfaces or type
  aliases in `*.utils.ts`; the architecture checker enforces this boundary.

## UI and accessibility

- Use Tailwind v4 tokens/CSS variables and existing Z-UI visual patterns; avoid new
  hardcoded colors or one-off wrapper components.
- Preserve keyboard, focus, disabled, readonly, loading, empty, error, dark-mode, and
  reduced-motion behavior whenever the component supports those states.
- Use semantic elements and native interaction first. Add ARIA only when native
  semantics are insufficient, and verify interactive roles are focusable.
- Prefer `motion-safe:`/`motion-reduce:` behavior and avoid animation that changes the
  component's functional state without an accessible equivalent.
- Keep HTML rendered with `{@html ...}` trusted or sanitized at the call boundary. A
  renderer must not silently turn untrusted user input into executable markup.

## Tests and verification

Test public behavior, not implementation details. For interactive components cover the
smallest meaningful matrix: default and edge inputs, keyboard/pointer behavior, state
transitions, disabled/readonly paths, dark or reduced-motion branches, and cleanup.
Use nearby Vitest patterns (`mount`, `flushSync`, `unmount`) and test pure geometry or
state logic directly in its `*.utils.spec.ts` file.

Run the narrowest checks first, then expand for shared changes:

```bash
pnpm --filter @shival99/z-ui-svelte check
pnpm --filter @shival99/z-ui-svelte test
pnpm nx run z-ui-svelte:check
node scripts/z-architecture-check.mjs <task-files...>
node scripts/z-lint-changed.mjs <task-files...>
pnpm skills:check
```

For a public component change, also verify its demo and exports. Keep the full Svelte
test suite green before commit; report pre-existing warnings separately from failures.
Do not weaken tests, hide diagnostics, or run a full repository lint solely because
unrelated files are already dirty.

## Delivery checklist

- The component, types, variants, utilities, exports, demo, docs/API rows, and tests
  are updated only when each is relevant to the actual public change.
- Current-task files pass architecture placement, formatting, and changed-file lint.
- The Svelte check and full Vitest suite pass.
- The final report names changed files, checks, commit/push state, and any remaining
  warning or unverified runtime behavior.

References: [Svelte runes](https://svelte.dev/docs/svelte/what-are-runes),
[Svelte testing](https://svelte.dev/docs/svelte/testing), and the repository's
canonical agent instructions.
