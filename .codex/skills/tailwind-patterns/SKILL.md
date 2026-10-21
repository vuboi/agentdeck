---
name: tailwind-patterns
description: Use for Tailwind v4 tokens, reusable variants, configuration, or non-trivial responsive utility architecture.
---

# Tailwind Patterns

1. Confirm the installed Tailwind version and local token/CVA conventions.
2. Reuse theme tokens and existing utilities; do not hardcode colors that bypass light/dark themes.
3. Keep one-off layout classes in templates. Use CVA for reusable visual variants with a real component API.
4. Merge public class inputs with the repo's established merge helper.
5. Design mobile-first, then add only breakpoints required by content behavior.
6. Avoid arbitrary values when a token exists, duplicated utility groups, and nested wrappers used only for sizing.
7. Verify focus, disabled, dark theme, long content, overflow, and key responsive widths in the browser.
