# Design pre-delivery checklist

Run the items relevant to the change. Mark unavailable checks as unverified with a reason; do not silently treat them as passed.

## Direction and product fit

- [ ] The user, primary job, visual concept, and signature are explicit.
- [ ] Existing tokens, app shell, nearby screens, and shared primitives were inspected.
- [ ] The direction is specific to the product and does not invent business behavior.
- [ ] Category reflexes and any justified exceptions are recorded.

## Visual quality

- [ ] Hierarchy is obvious in the first viewport and at the intended density.
- [ ] Typography, spacing, color, radius, borders, and iconography use existing tokens or a deliberate local extension.
- [ ] Copy names user actions and explains empty/error states without filler or fabricated proof.
- [ ] Decorative elements have a job; remove anything that does not improve comprehension or identity.

## Responsive and browser evidence

- [ ] Rendered output was checked at 375px, 768px, and 1440px when the app could run.
- [ ] An extra narrow check at 320px or 414px was made when the target is dense or text-sensitive.
- [ ] No clipped content, accidental horizontal scroll, broken wrapping, or unusable touch target is present.
- [ ] Assets load, important text remains readable, and the first viewport does not feel like a sealed poster when continuation is expected.
- [ ] Console errors, failed network requests, blank regions, and obvious runtime warnings were investigated.

## Interaction and accessibility

- [ ] Touched controls have appropriate default, hover, focus-visible, active, disabled, loading, empty, error, and success states where relevant.
- [ ] Keyboard order and operation are usable; focus is visible and never communicated by color alone.
- [ ] Inputs have labels, custom controls have correct semantics, and important feedback has text or an equivalent non-color signal.
- [ ] Reduced motion is honored and no interaction causes avoidable layout shift.

## Engineering fit

- [ ] Existing Angular/Nx ownership, Tailwind/CVA conventions, i18n, and Z-UI APIs were preserved.
- [ ] No parallel component library, token system, or wrapper was introduced without a concrete ownership reason.
- [ ] Relevant lint, type, test, architecture, and runtime checks were run.
