export type TerminalTab = { id: string; name: string; lines: string[] };

// Mock terminal state. Real PTY sessions arrive with agentd in M2.
let counter = 1;

export const terminal = $state({
  open: true,
  active: 'term-1',
  tabs: [{ id: 'term-1', name: 'zsh', lines: ['AgentDeck terminal (preview, no PTY yet)'] }] as TerminalTab[],
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
  if (event.key === '`' && event.shiftKey) {
    newTerminal();
  } else if (event.key === '`') {
    toggleTerminal();
  } else if (event.key === 'j' && !event.shiftKey) {
    toggleTerminal();
  } else if (event.key === 'W' && event.shiftKey) {
    closeTerminal();
  } else if (event.key === 'PageDown') {
    cycleTerminal(1);
  } else if (event.key === 'PageUp') {
    cycleTerminal(-1);
  } else {
    return false;
  }
  event.preventDefault();
  return true;
}
