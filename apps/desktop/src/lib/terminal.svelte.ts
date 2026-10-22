import { getPref, setPref } from './prefs';

export type TerminalTab = { id: string; name: string; lines: string[] };

// Mock terminal state. Real PTY sessions arrive with agentd in M2.
let counter = 1;

export const terminal = $state({
  // Hidden by default: it opens when asked (the shortcut, or a new terminal). A saved choice still wins.
  open: false,
  active: 'term-1',
  tabs: [{ id: 'term-1', name: 'zsh', lines: ['AgentDeck terminal (preview, no PTY yet)'] }] as TerminalTab[],
});

// Open or closed, the tabs and the active one are kept in IndexedDB: a reload brings the terminal back as it was.
type SavedTerminal = { open: boolean; active: string; tabs: TerminalTab[] };
let terminalHydrated = false;
Promise.all([getPref<SavedTerminal>('terminal.state'), getPref<boolean>('terminal.open')]).then(([saved, legacyOpen]) => {
  if (saved && Array.isArray(saved.tabs) && saved.tabs.length > 0) {
    terminal.tabs = saved.tabs;
    terminal.active = saved.tabs.some((tab) => tab.id === saved.active) ? saved.active : saved.tabs[0].id;
    terminal.open = saved.open === true;
  } else if (typeof legacyOpen === 'boolean') {
    terminal.open = legacyOpen;
  }
  terminalHydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot: SavedTerminal = {
      open: terminal.open,
      active: terminal.active,
      tabs: terminal.tabs.map((tab) => ({ id: tab.id, name: tab.name, lines: tab.lines.slice(-500) })),
    };
    if (terminalHydrated) setPref('terminal.state', snapshot);
  });
});

export function toggleTerminal() {
  terminal.open = !terminal.open;
}

export function newTerminal() {
  counter += 1;
  const id = `term-${counter}`;
  terminal.tabs.push({ id, name: 'zsh', lines: [] });
  terminal.active = id;
  terminal.open = true;
}

export function closeTerminal(id = terminal.active) {
  const index = terminal.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  terminal.tabs.splice(index, 1);
  if (terminal.tabs.length === 0) {
    terminal.open = false;
    return;
  }
  if (terminal.active === id) {
    terminal.active = terminal.tabs[Math.max(0, index - 1)].id;
  }
}

export function cycleTerminal(step: 1 | -1) {
  const index = terminal.tabs.findIndex((tab) => tab.id === terminal.active);
  const next = (index + step + terminal.tabs.length) % terminal.tabs.length;
  terminal.active = terminal.tabs[next].id;
}

/** VS Code-style shortcuts. Returns true when the event was handled. */
export function handleTerminalShortcut(event: KeyboardEvent): boolean {
  const mod = event.ctrlKey || event.metaKey;
  if (!mod) return false;

  switch (event.key) {
    case '`':
      if (event.shiftKey) newTerminal();
      else toggleTerminal();
      break;
    case 'j':
      if (event.shiftKey) return false;
      toggleTerminal();
      break;
    case 'W':
      if (!event.shiftKey) return false;
      closeTerminal();
      break;
    case 'PageDown':
      cycleTerminal(1);
      break;
    case 'PageUp':
      cycleTerminal(-1);
      break;
    default:
      return false;
  }

  event.preventDefault();
  return true;
}
