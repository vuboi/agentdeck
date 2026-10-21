# Brand and product grounding

Use this reference when the target has no obvious visual direction or when a redesign risks replacing useful product language.

## Source order

1. Read the project's brand or product brief, design docs, and existing tokens.
2. Inspect the app shell and 1–2 nearby screens that solve a similar user problem.
3. Inspect shared components, typography, iconography, and spacing conventions.
4. Use a user-provided screenshot, Figma context, or public reference only as evidence for the requested work.

The existing product is the source of truth for behavior and reusable primitives. A reference can inform composition and tone; it is not permission to copy content, pixels, or a competitor's identity.

## If the brand is underspecified

Write a provisional note before implementation:

- three voice adjectives;
- one sentence describing the user and primary job;
- a restrained palette strategy with existing tokens or named values;
- a type stance and density level;
- one signature choice and one category cliché to avoid.

Keep the assumptions local to the task. Do not create global tokens, fonts, assets, or brand files unless the user asks to establish a product-wide system.

## Existing product versus greenfield

For an existing screen, keep what already works: terminology, navigation, layout conventions, permissions, state behavior, and established primitives. Replace the weakest visible decisions first.

For a greenfield screen, make the subject matter visible in the first viewport and let the domain determine the structure. A CRM, editor, component demo, and marketing page should not converge on the same hero, card grid, or density.

## Angular and Z-UI guardrails

Use the app's Angular/Nx boundaries, Tailwind/CVA patterns, i18n contract, and existing Z-UI components. Do not introduce a parallel token system or wrapper component just to express a visual preference. If the current library prevents a necessary interaction or state, document the gap and fix the owning layer.
