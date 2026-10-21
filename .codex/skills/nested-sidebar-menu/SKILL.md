---
name: nested-sidebar-menu
description: Use when changing Z-UI nested sidebars, collapsed popovers, mobile drawers, active-route expansion, or submenu animation.
---

# Nested Sidebar Menu

1. Inspect the component, tree types, route-selection logic, demo, and one existing reference before editing.
2. Keep logical expansion separate from render/transition state. Mount children before opening; on close, remove the expanded class before unmounting. Clear pending timers on destroy.
3. Use one consistent DOM/animation structure across levels. Prefer a `0fr` to `1fr` grid transition with an inner `min-height: 0; overflow: hidden` wrapper.
4. Keep vertical padding, margins, and static gaps inside the clipped content; spacing outside the animated panel causes layout jumps.
5. Initial route sync should run after the processed tree is ready, select the best exact/subpath/deep-prefix match, and expand its parent chain once. Do not unexpectedly reopen parents on later navigation.
6. Distinguish a selected leaf from a parent containing the selected leaf: leaf gets the full active surface; parent usually gets active foreground while retaining neutral hover behavior.
7. Collapsed popovers must cap height by viewport, support mouse-wheel scrolling, keep the scrollbar edge-aligned, and place item padding inside scroll content.
8. Mobile drawers should not duplicate a host layout's top bar and must preserve the same active/expand semantics.
9. Cover multiple-open, accordion, collapsed popover, long menu, deep nesting, refresh, and mobile modes.
10. Run focused lint/type checks and visually verify symmetric animation, stable spacing, scrolling, active state, and cleanup after rapid toggles.
