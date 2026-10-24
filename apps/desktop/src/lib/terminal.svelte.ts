import { getPref, setPref } from './prefs';
import { disposeSession, writeToSession, type SessionStatus } from './terminal-session';

export type TerminalTab = { id: string; name: string; status: SessionStatus };

let counter = 1;

export const terminal = $state({
  // Hidden by default: it opens when asked (the shortcut, or a new terminal). A saved choice still wins.
  open: false,
  active: 'term-1',
  tabs: [{ id: 'term-1', name: 'shell', status: 'starting' }] as TerminalTab[],
});

// Open or closed, the tabs and the active one are kept in IndexedDB: a reload brings the terminal back as it was.
// A shell does not survive a reload, so a tab comes back with a new one in it (older saves also had its lines).
type SavedTerminal = { open: boolean; active: string; tabs: { id: string; name: string }[] };
let terminalHydrated = false;
Promise.all([getPref<SavedTerminal>('terminal.state'), getPref<boolean>('terminal.open')]).then(([saved, legacyOpen]) => {
  if (saved && Array.isArray(saved.tabs) && saved.tabs.length > 0) {
    terminal.tabs = saved.tabs.map((tab) => ({ id: tab.id, name: tab.name, status: 'starting' as SessionStatus }));
    terminal.active = saved.tabs.some((tab) => tab.id === saved.active) ? saved.active : saved.tabs[0].id;
    terminal.open = saved.open === true;
    // New tabs must not take an id a restored one has.
    counter = Math.max(counter, ...saved.tabs.map((tab) => Number(tab.id.replace('term-', '')) || 0));
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
      tabs: terminal.tabs.map((tab) => ({ id: tab.id, name: tab.name })),
    };
    if (terminalHydrated) setPref('terminal.state', snapshot);
  });
});

export function toggleTerminal() {
  terminal.open = !terminal.open;
}

/** Runs a command from a chat's code block in the active terminal tab, opening the panel if it is closed. */
export function runInTerminal(command: string) {
  terminal.open = true;
  const lines = command.trimEnd().split(/\r?\n/);
  writeToSession(terminal.active, lines.join('\r') + '\r');
}

export function newTerminal() {
  counter += 1;
  const id = `term-${counter}`;
  terminal.tabs.push({ id, name: 'shell', status: 'starting' });
  terminal.active = id;
  terminal.open = true;
}

export function closeTerminal(id = terminal.active) {
  const index = terminal.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  disposeSession(id);
  terminal.tabs.splice(index, 1);
  if (terminal.tabs.length === 0) {
    terminal.open = false;
    // The next time it opens it has a tab again, with a new id: the closed one's shell is gone.
    counter += 1;
    terminal.tabs.push({ id: `term-${counter}`, name: 'shell', status: 'starting' });
    terminal.active = terminal.tabs[0].id;
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
