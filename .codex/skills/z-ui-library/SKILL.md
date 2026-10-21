---
name: z-ui-library
description: Use when building or updating UI with `@shival99/z-ui` APIs, components, directives, services, refs, or public patterns.
---

# Z-UI Library

1. Use CodeGraph to find 1-2 similar local usages.
2. Run `pnpm z-ui:api <component>` to verify exact installed inputs/models/outputs.
3. Prefer existing Z-UI primitives and companion directives/services/refs over wrappers or replacements.
4. Match local CVA, variants, Tailwind, naming, and typed-config patterns.
5. Use `zMergeClasses` when merging class inputs; use typed `zControl` for imperative hooks.
6. Never read the full catalog for API details or invent an input/output/model.
7. `z-ui:catalog` regenerates only a lightweight discovery index after upgrading Z-UI.

Do not create local replacements, duplicate generic contracts, or many loosely related flags when an existing typed configuration/pattern works.
