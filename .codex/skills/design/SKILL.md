---
name: design
description: 'Use for non-trivial frontend UI work: designing, building, redesigning, polishing, auditing, or visually verifying pages, flows, components, and app shells. Enforces product-grounded direction, anti-slop judgment, and browser evidence.'
---

# Design

This is the judgment layer for frontend work. It decides what the interface should feel like, what deserves visual emphasis, and whether the rendered result is convincing. It is not a component library, Tailwind catalog, or replacement for the project's design system.

The skill is a focused local adaptation of ideas from Anthropic's `frontend-design` and sboghossian's `design-skill`, integrated with the repository's Angular, Tailwind, Z-UI, and browser tooling conventions.

## Scope gate

Use the full workflow for a new screen, landing page, app shell, substantial redesign, visual polish pass, component-system change, or any UI change that affects hierarchy, layout, interaction states, or responsive behavior.

For a tiny UI change that only adjusts a local class or copy string, perform a quick fit check against the existing screen and skip the full direction document. Do not activate this workflow for backend-only, data-only, or mechanical refactors with no visible behavior.

## Workflow

### 1. Ground in the product

Before writing markup, inspect the target feature and nearby screens. Look for these sources of truth in order:

- `brand-spec.md`, `BRAND.md`, `DESIGN.md`, `PRODUCT.md`, or relevant docs;
- existing Tailwind/theme/global style tokens and typography;
- the app shell, shared layout, Z-UI primitives, and 1–2 nearby screens;
- user-provided screenshots, Figma context, or reference URLs.

Identify the user, the screen's primary job, the information hierarchy, and the existing states. If no brand source exists, state provisional assumptions in the direction line; do not silently invent a parallel design system.

For Z-UI work, inspect the local demos and installed public API before changing component usage. Existing tokens, public APIs, and behavior remain authoritative.

### 2. Commit to a direction before code

Write a compact design read before implementation. It must name:

- subject, audience, and the screen's single job;
- one concrete visual concept and one memorable signature;
- color strategy: restrained, committed, full palette, or drenched;
- type stance: utility, editorial, classical pair, mono-display, or another justified choice;
- density, motion, and variance dials from 1–10;
- the obvious category cliché you are deliberately avoiding or the reason an exception is necessary.

For a page, add a one-sentence layout concept or small wireframe. For a component, add a state map. If the direction could describe any product in the same category, make it more specific before coding.

### 3. Run the anti-slop and category check

Read [references/anti-slop.md](references/anti-slop.md) when choosing the direction. Refuse reflexes such as Inter/system fonts everywhere, unearned purple-blue gradients, identical three-card grids, generic centered SaaS heroes, card nesting without purpose, decorative KPI blocks, low-contrast gray text, and hover-only primary actions.

An exception is allowed only when the existing product or the domain genuinely requires it. Record the reason in the direction line and keep the rest of the composition restrained.

### 4. Build the usable interface

- Preserve the existing Angular/Nx structure, Tailwind/CVA conventions, tokens, and component ownership.
- Use real domain copy and assets. Never fabricate metrics, testimonials, logos, statuses, permissions, or API semantics to fill a design.
- Make hierarchy visible through type, spacing, alignment, grouping, and content order before adding decoration.
- Design the states relevant to touched controls: default, hover, focus-visible, active, disabled, loading, empty, error, and success where applicable.
- Keep important actions visible without hover, use semantic controls, and keep keyboard flow and accessible names intact.
- Start mobile-first. Prevent clipping and accidental horizontal scroll; make responsive changes intentional rather than breakpoint patches.
- Use motion only to explain state or orientation, honor reduced motion, and prefer one coherent moment over scattered effects.

### 5. Verify the rendered result

For substantial UI work, run the affected app when practical and inspect the actual browser output. Use the repository's `realbrowser` skill for the user's existing Chrome context and Chrome DevTools MCP for isolated runtime evidence. Do not claim visual quality from source inspection alone.

Check at minimum:

- 375px mobile, 768px tablet, and 1440px desktop;
- 320px or 414px as an extra narrow check when the layout has dense controls, long labels, or fixed-format content;
- first viewport hierarchy, wrapping, alignment, overflow, contrast, and asset loading;
- focus, hover, active, disabled, loading, empty, and error states that were touched;
- browser console errors, failed network requests, and blank or partially rendered regions.

Fix visible defects and repeat the relevant check. When browser verification is unavailable, say exactly what could not be verified and do not use “looks great” as evidence.

### 6. Ship through the checklist

Before handoff, read [references/checklist.md](references/checklist.md). Report the direction, screenshots or browser checks performed, and any honest exemptions. Keep the checklist proportional to the change, but never silently skip accessibility, responsive, or runtime evidence that was relevant.

## Skill coordination

- Use `hallmark` for deep structural variety, redesign/study flows, macrostructure selection, and its staged anti-slop references.
- Use `design-taste-frontend` for repo-native Angular implementation and interaction quality when it is available in the project.
- Use `z-ui-library` for Z-UI API/component work.
- Use `realbrowser` or Chrome DevTools according to the project contract; do not add a new browser dependency just for this skill.

The design direction must fit the product. Taste is not a license to replace established information architecture, business behavior, or a working design system.
