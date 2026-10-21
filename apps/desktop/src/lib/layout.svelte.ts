// Shared shell state. The page reacts to `toolsOpen` by resizing the columns.
// `themeAnchor` is the palette button on screen, so the theme popover has a position the moment it opens.
export const layout = $state({ toolsOpen: true, themePanelOpen: false, themeAnchor: null as HTMLElement | null });

export function toggleTools() {
  layout.toolsOpen = !layout.toolsOpen;
}

export function toggleThemePanel() {
  layout.themePanelOpen = !layout.themePanelOpen;
}
