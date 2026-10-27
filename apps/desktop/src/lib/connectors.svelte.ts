import { inTauri } from './history';
import { getPref, setPref } from './prefs';
import { toast } from './toast';
import { requestSystemNotificationPermission, sendSystemNotification } from './system-notifications';

// Settings → Connectors. State and actions for: the agent CLIs (installed? signed in?), the accounts each one can
// switch between (Codex via `codex-auth`, Claude via `cswap`), the "close to the limit" alerts, and custom / local
// model providers. The work itself is in Rust (`connectors.rs`); this file holds what the screen shows.

export type ConnectorId = 'claude' | 'codex' | 'agy' | 'opencode';
export type AccountProvider = 'claude' | 'codex';

export type Tool = { id: string; path: string | null; version: string | null };
export type Login = { logged_in: boolean | null; email: string | null; plan: string | null; detail: string | null };
export type Detected = { tools: Tool[]; claude: Login; codex: Login; antigravity: Login; opencode: Login };
export type UsageWindow = { label: string; pct: number; resets_at: number | null };
export type Account = {
  id: string;
  email: string;
  alias: string | null;
  plan: string | null;
  active: boolean;
  usage: UsageWindow[];
  observed_at: number | null;
  status: string | null;
  /** The login ran out (a 401): sign in again; the account cannot be switched to meanwhile. */
  expired: boolean;
  /** Live count returned by ChatGPT's reset-credit API; absent when that API could not be read. */
  reset_credits_available?: number;
};
export type Accounts = {
  manager: boolean;
  accounts: Account[];
  auto_switch: { enabled: boolean; five_hour: number; weekly: number } | null;
  error: string | null;
};
export type QuotaGroup = { id: string; label: string; windows: UsageWindow[] };
export type AgyQuota = { observed_at: number | null; groups: QuotaGroup[] };
export type AgyAccount = { id: string; email: string; active: boolean; expired: boolean; status: string | null };
export type ProviderKind = 'openai' | 'anthropic' | 'ollama';
export type CustomProvider = {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  /** Optional environment variable to read the key from when no key is typed. */
  keyEnv: string;
  /** The API key. Kept only in this app's local storage on this computer; never part of the source or a build. */
  apiKey?: string;
  /** A logo the user uploaded, as a small `data:` image. Without one the provider shows a server icon. */
  logo?: string;
  models: string[];
  defaultModel: string;
  enabled: boolean;
  /** A free tier that may keep and train on what it is sent: it never sees project files, code or secrets. */
  restricted?: boolean;
};
/** The Claude Console's API credit: the key the `credit:` Claude models run on, and the Console sign-in that reads
 * the credit left. Both stay on this computer only. */
export type ClaudeCredit = { apiKey: string; sessionKey: string; enabled: boolean };
export type Probe = { ok: boolean; models: string[]; ms: number; error: string | null };
export type LocalServer = { kind: ProviderKind; label: string; base_url: string; models: string[] };

export type Section = 'connectors' | 'providers' | 'alerts' | 'sync' | 'computer';

export const settings = $state({ open: false, section: 'connectors' as Section });

export function openSettings(section: Section = 'connectors') {
  settings.section = section;
  settings.open = true;
}

export const connectors = $state({
  detected: null as Detected | null,
  accounts: { claude: null, codex: null, agy: [] } as Record<AccountProvider, Accounts | null> & { agy: AgyAccount[] },
  /** What is running right now, by key (`install:codex`, `login:claude`, `switch:codex`…), with a label for it. */
  busy: {} as Record<string, string>,
  /** Last lines of an install or sign-in, by connector id. */
  logs: {} as Record<string, string[]>,
  /** The last error of an action, by key. */
  errors: {} as Record<string, string>,
  alerts: { enabled: true, threshold: 90, system: true, systemChosen: false },
  /** When a turn fails: retry, or let the next agent (provider) of `chain` carry on; never another account (turn-failure.ts). */
  recovery: { enabled: false, chain: ['claude', 'codex', 'opencode', 'antigravity'] as string[], /** At a usage limit with nowhere to move, carry on by itself once the limit resets. */ autoResume: true, /** The user set `enabled` themself (it was on by default before). */ chosen: false },
  custom: [] as CustomProvider[],
  /** Claude Console API credit: Claude Code runs on this key (billed to the credit) for the `credit:` models. */
  claudeCredit: { apiKey: '', sessionKey: '', enabled: true } as ClaudeCredit,
  local: [] as LocalServer[],
  /** What agy last reported per model family, read from the file a status-line tool saves. `null`: none saved. */
  agyQuota: null as AgyQuota | null,
  scanning: false,
  manualRefreshing: false,
});

const activeLoginRuns = new Map<ConnectorId, number>();
const cancelledLoginRuns = new Set<number>();
const activeAccountAddRuns = new Map<AccountProvider, number>();
const cancelledAccountAddRuns = new Set<number>();
let loginSequence = 0;

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
};

/** Stops a connector screen from waiting forever if its native command never answers. */
const callWithTimeout = async <T>(command: string, args: Record<string, unknown> | undefined, timeoutMs: number): Promise<T> => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      call<T>(command, args),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${command} did not respond within ${Math.round(timeoutMs / 1000)} seconds`)), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
};

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Runs an action with a busy label and a captured error. Returns whether it worked. */
async function task(key: string, label: string, action: () => Promise<unknown>): Promise<boolean> {
  if (connectors.busy[key]) return false;
  connectors.busy[key] = label;
  delete connectors.errors[key];
  try {
    await action();
    return true;
  } catch (error) {
    connectors.errors[key] = message(error);
    return false;
  } finally {
    delete connectors.busy[key];
  }
}

export const tool = (id: string) => connectors.detected?.tools.find((item) => item.id === id);
export const installed = (id: string) => Boolean(tool(id)?.path);

let listening = false;
async function listenToLogs() {
  if (listening || !inTauri()) return;
  listening = true;
  const { listen } = await import('./tauri-event');
  await listen<{ id: string; line: string }>('connector-log', ({ payload }) => {
    const lines = (connectors.logs[payload.id] ??= []);
    lines.push(payload.line);
    if (lines.length > 60) lines.splice(0, lines.length - 60);
  });
}

export async function refreshDetect() {
  if (!inTauri()) return;
  void listenToLogs();
  await task('detect', 'Checking', async () => {
    connectors.detected = await callWithTimeout<Detected>('connectors_detect', undefined, 30_000);
  });
}

let adopted = false;

export async function refreshAccounts(provider: AccountProvider, refresh = false) {
  if (!inTauri()) return;
  const timeoutMs = provider === 'claude' ? 90_000 : 65_000;
  await task(`accounts:${provider}`, refresh ? 'Refreshing usage' : 'Reading accounts', async () => {
    const next = await callWithTimeout<Accounts>('accounts_list', { provider, refresh }, timeoutMs);
    // A fresh `cswap` knows no account yet: save the one Claude Code is signed in as, so the list is not empty.
    if (provider === 'claude' && next.manager && next.accounts.length === 0 && !next.error && connectors.detected?.claude.logged_in && !adopted) {
      adopted = true;
      try {
        await call('account_adopt');
        Object.assign(next, await callWithTimeout<Accounts>('accounts_list', { provider, refresh }, timeoutMs));
      } catch (error) {
        next.error = `Could not save the current Claude account: ${message(error)}`;
      }
    }
    connectors.accounts[provider] = next;
  });
}

export async function refreshAgyQuota() {
  if (!inTauri()) return;
  // The local status-line cache is immediate; do not leave the Usage cards blank while the live API request waits.
  if (!connectors.agyQuota) {
    try {
      connectors.agyQuota = await call<AgyQuota | null>('antigravity_quota_cached');
    } catch {
      // Keep going: the live request may still return current figures.
    }
  }
  try {
    const live = await call<AgyQuota | null>('antigravity_quota');
    if (live) connectors.agyQuota = live;
  } catch {
    // Preserve saved figures when the live quota endpoint is unavailable.
  }
}

let agyAccountsRefresh: Promise<boolean> | undefined;

export async function refreshAgyAccounts() {
  if (!inTauri()) return;
  if (agyAccountsRefresh) return agyAccountsRefresh;
  const refresh = task('accounts:agy', 'Reading Antigravity accounts', async () => {
    connectors.accounts.agy = await call<AgyAccount[]>('antigravity_accounts_list');
  });
  agyAccountsRefresh = refresh;
  try {
    await refresh;
  } finally {
    if (agyAccountsRefresh === refresh) agyAccountsRefresh = undefined;
  }
}

/** Retries saving the currently signed-in Antigravity account after automatic adoption failed. */
export async function retrySaveAgyAccount(): Promise<boolean> {
  const ok = await task('save:agy', 'Saving Antigravity account', async () => {
    await call('antigravity_account_adopt');
    connectors.accounts.agy = await call<AgyAccount[]>('antigravity_accounts_list');
  });
  if (ok) {
    delete connectors.errors['login:agy'];
    toast.success('Antigravity account saved');
  }
  return ok;
}

export async function switchAgyAccount(account: AgyAccount): Promise<boolean> {
  const ok = await task('switch:agy', `Switching to ${account.email}`, () => call('antigravity_account_switch', { id: account.id }));
  if (ok) {
    toast.success(`Now using ${account.email}`);
    await Promise.all([refreshAgyAccounts(), refreshDetect(), refreshAgyQuota()]);
  }
  return ok;
}

export async function refreshAll(refresh = false) {
  if (refresh && connectors.manualRefreshing) return;
  if (refresh) connectors.manualRefreshing = true;
  try {
    // Keep Claude detection first because account adoption depends on its signed-in state.
    // Codex and Antigravity account reads can proceed while CLI detection runs.
    void refreshAgyQuota();
    await Promise.all([refreshDetect(), refreshAccounts('codex', refresh), refreshAgyAccounts()]);
    await refreshAccounts('claude', refresh);
  } finally {
    if (refresh) connectors.manualRefreshing = false;
  }
}

export async function install(id: string, label: string) {
  connectors.logs[id] = [];
  const ok = await task(`install:${id}`, `Installing ${label}`, () => call('connector_install', { id }));
  if (ok) {
    toast.success(`${label} installed`);
    await refreshAll();
  }
}

export async function login(id: ConnectorId, label: string): Promise<boolean> {
  if (connectors.busy[`login:${id}`]) return false;
  const run = ++loginSequence;
  activeLoginRuns.set(id, run);
  connectors.logs[id] = [];
  const ok = await task(`login:${id}`, `Waiting for ${label} sign-in`, () => call('connector_login', { id }));
  const cancelled = cancelledLoginRuns.delete(run);
  if (activeLoginRuns.get(id) === run) activeLoginRuns.delete(id);
  if (!ok && cancelled) {
    delete connectors.errors[`login:${id}`];
    return false;
  }
  if (ok) {
    if (id === 'claude') adopted = false; // a fresh sign-in may fix what stopped the account being saved
    // Login completion is independent from slow usage/account reads. Refresh those in the background.
    void refreshAll();
    if (id !== 'agy') toast.success(`${label} signed in`);
  }
  return ok;
}

/** Cancels a connector sign-in started from an account confirmation dialog. */
export async function cancelLogin(id: ConnectorId): Promise<void> {
  const run = activeLoginRuns.get(id);
  if (run !== undefined) cancelledLoginRuns.add(run);
  try {
    await call('connector_login_cancel', { id });
  } catch {
    if (run !== undefined) cancelledLoginRuns.delete(run);
  }
}

export async function addAccount(provider: AccountProvider, label: string) {
  if (connectors.busy[`add:${provider}`]) return;
  const run = ++loginSequence;
  activeAccountAddRuns.set(provider, run);
  connectors.logs[provider] = [];
  const ok = await task(`add:${provider}`, `Adding a ${label} account`, () => call('account_add', { provider }));
  const cancelled = cancelledAccountAddRuns.delete(run);
  if (activeAccountAddRuns.get(provider) === run) activeAccountAddRuns.delete(provider);
  if (!ok && cancelled) {
    delete connectors.errors[`add:${provider}`];
    return;
  }
  if (ok) {
    toast.success(`${label} account added`);
    await refreshAll();
  }
}

/** Cancels an in-progress account-add flow and suppresses its expected cancellation error. */
export async function cancelAccountAdd(provider: AccountProvider): Promise<void> {
  const run = activeAccountAddRuns.get(provider);
  if (run !== undefined) cancelledAccountAddRuns.add(run);
  try {
    await call('account_add_cancel', { provider });
  } catch {
    if (run !== undefined) cancelledAccountAddRuns.delete(run);
  }
}

export async function switchAccount(provider: AccountProvider, account: Account): Promise<boolean> {
  const ok = await task(`switch:${provider}`, `Switching to ${account.email}`, () => call('account_switch', { provider, id: account.id }));
  if (ok) {
    toast.success(`Now using ${account.email}`);
    await refreshAll();
  }
  return ok;
}

export async function removeAccount(provider: AccountProvider, account: Account) {
  const ok = await task(`remove:${provider}`, 'Removing', () => call('account_remove', { provider, id: account.id }));
  if (ok) {
    toast.success(`${account.email} removed`);
    await refreshAll();
  }
}

export async function setCodexAutoSwitch(enabled: boolean, fiveHour: number, weekly: number) {
  const ok = await task('auto:codex', 'Saving', () => call('codex_auto_switch', { enabled, fiveHour, weekly }));
  if (ok) await refreshAccounts('codex');
}

/** Redeems one Codex reset credit after the user confirms the action. */
export async function consumeCodexResetCredit(email: string): Promise<boolean> {
  const key = `reset:${email}`;
  const ok = await task(key, 'Redeeming Codex reset', () => call('codex_consume_reset_credit', { email }));
  if (!ok) return false;
  toast.success('Codex reset redeemed');
  // The button keeps spinning until the new usage is shown, not only while the reset is redeemed.
  connectors.busy[key] = 'Updating usage';
  try {
    await refreshAccounts('codex', true);
  } finally {
    delete connectors.busy[key];
  }
  return true;
}

// --- Alerts: tell the user before an account runs out -------------------------------------------------------

export type Warning = { provider: AccountProvider; account: Account; window: UsageWindow; better: Account | null };

const NAMES: Record<AccountProvider, string> = { claude: 'Claude', codex: 'Codex' };
export const providerName = (provider: AccountProvider) => NAMES[provider];

/** Windows of the active account that are at or past the threshold and not yet reset. */
export function warnings(now = Date.now()): Warning[] {
  const found: Warning[] = [];
  for (const provider of ['claude', 'codex'] as const) {
    const list = connectors.accounts[provider]?.accounts ?? [];
    const active = list.find((account) => account.active);
    if (!active) continue;
    for (const window of active.usage) {
      const live = window.resets_at === null || window.resets_at * 1000 > now;
      if (!live || window.pct < connectors.alerts.threshold) continue;
      const better = list
        .filter((account) => !account.active)
        .filter((account) => account.usage.length > 0 && account.usage.every((item) => item.pct < connectors.alerts.threshold || (item.resets_at !== null && item.resets_at * 1000 <= now)))
        .sort((a, b) => Math.max(...a.usage.map((item) => item.pct)) - Math.max(...b.usage.map((item) => item.pct)))[0];
      found.push({ provider, account: active, window, better: better ?? null });
    }
  }
  return found;
}

/** A usage window that still counts now (not reset yet). */
const liveWindow = (window: UsageWindow, now: number) => window.resets_at === null || window.resets_at * 1000 > now;
/** An account that can take work now: signed in, and no live window full. */
const hasRoom = (account: Account, now: number) =>
  !account.expired && account.usage.every((window) => !liveWindow(window, now) || window.pct < 98);

/**
 * Whether an agent can take work now, as far as the app knows: the account it is signed in with (Claude, Codex) is
 * not full or signed out. Other accounts do not count: the app never switches accounts by itself. An agent whose
 * usage the app cannot read counts as available.
 */
export function agentHasRoom(agent: string, now = Date.now()): boolean {
  if (agent !== 'claude' && agent !== 'codex') return true;
  const active = connectors.accounts[agent]?.accounts.find((account) => account.active);
  return !active || hasRoom(active, now);
}

/** When the signed-in account of an agent is free again: the latest reset among its full windows, in ms; null when none is full. */
export function limitResetAt(agent: string, now = Date.now()): number | null {
  if (agent !== 'claude' && agent !== 'codex') return null;
  const active = connectors.accounts[agent]?.accounts.find((account) => account.active);
  const full = active?.usage.filter((window) => window.pct >= 98 && window.resets_at !== null && window.resets_at * 1000 > now) ?? [];
  return full.length ? Math.max(...full.map((window) => window.resets_at as number)) * 1000 : null;
}

/** One line on an agent's room for `list_agents`: "5h 42%, weekly 80%" or "signed out". */
export function roomLine(agent: string, now = Date.now()): string {
  if (agent !== 'claude' && agent !== 'codex') return '';
  const active = connectors.accounts[agent]?.accounts.find((account) => account.active);
  if (!active) return '';
  if (active.expired) return 'signed out';
  const windows = active.usage.filter((window) => liveWindow(window, now)).map((window) => `${window.label} ${Math.round(window.pct)}%`);
  return windows.length ? `usage ${windows.join(', ')}` : '';
}

const announced = new Set<string>();

function announce(warning: Warning) {
  const key = `${warning.provider}|${warning.account.email}|${warning.window.label}|${warning.window.resets_at}`;
  if (announced.has(key)) return;
  announced.add(key);
  const name = providerName(warning.provider);
  const hint = warning.better ? `Switch to ${warning.better.email} in Settings.` : 'No other account has room: add one in Settings.';
  const text = `${name} ${warning.window.label} limit at ${Math.round(warning.window.pct)}% on ${warning.account.email}. ${hint}`;
  toast.error(text);
  if (connectors.alerts.system) void sendSystemNotification(`${name} is close to its limit`, text);
}

export async function allowSystemNotifications(allow: boolean) {
  connectors.alerts.system = allow && (await requestSystemNotificationPermission());
  connectors.alerts.systemChosen = true;
}

let watching = false;
/** Reads the accounts every few minutes and raises a warning when the active one nears a limit. */
export function startWatching() {
  if (watching || !inTauri()) return;
  watching = true;
  const check = async () => {
    if (!connectors.alerts.enabled) return;
    if (!connectors.detected) await refreshDetect();
    const asked = (provider: AccountProvider) => (provider === 'codex' ? installed('codex-auth') : installed('cswap'));
    const targets = (['codex', 'claude'] as const).filter(asked);
    await Promise.all(targets.map((provider) => refreshAccounts(provider, true)));
    for (const warning of warnings()) announce(warning);
  };
  setTimeout(() => void check(), 20_000);
  setInterval(() => void check(), 5 * 60_000);
}

// --- Custom and local providers ----------------------------------------------------------------------------

/** What a provider says about the spending left on a key (OpenRouter), by provider id. */
export type ProviderQuota = { used: number; limit: number | null; remaining: number | null; free_tier: boolean; at: number };
export const providerQuotas = $state<Record<string, ProviderQuota>>({});

/** Reads the spending left on a custom provider's key, when the provider tells it; leaves it unset otherwise. */
export async function refreshProviderQuota(provider: CustomProvider) {
  if (!inTauri()) return;
  try {
    const found = await call<Omit<ProviderQuota, 'at'> | null>('provider_quota', { baseUrl: provider.baseUrl, apiKey: provider.apiKey || null, keyEnv: provider.keyEnv || null });
    if (found) providerQuotas[provider.id] = { ...found, at: Date.now() / 1000 };
    else delete providerQuotas[provider.id];
  } catch {
    // No reading: the footer then shows what the app counted itself.
  }
}

/** The `credit:` Claude models are offered: an API key is saved and turned on. */
export const claudeCreditOn = () => connectors.claudeCredit.enabled && connectors.claudeCredit.apiKey.trim() !== '';

/** What the Console says is left of the API credit (USD), or why it could not tell. */
export type ConsoleCredit = { balance: number; granted: number | null; expires_at: string | null; at: number };
export const claudeCreditState = $state<{ reading: ConsoleCredit | null; error: string | null; busy: boolean; top: number }>({ reading: null, error: null, busy: false, top: 0 });
void getPref<number>('providers.claudeCreditTop').then((top) => {
  if (top) claudeCreditState.top = Math.max(claudeCreditState.top, top);
});

/** Reads the credit left from the Console (the sign-in cookie pasted in Settings). The bar's full width is the
 * credit granted when the Console lists it, else the most this credit was ever seen at. */
export async function refreshClaudeCredit() {
  const key = connectors.claudeCredit.sessionKey.trim();
  if (!inTauri() || !key || claudeCreditState.busy) return;
  claudeCreditState.busy = true;
  try {
    const found = await call<Omit<ConsoleCredit, 'at'>>('claude_console_credit', { sessionKey: key });
    claudeCreditState.reading = { ...found, at: Date.now() / 1000 };
    claudeCreditState.error = null;
    const top = Math.max(claudeCreditState.top, found.granted ?? 0, found.balance);
    if (top !== claudeCreditState.top) {
      claudeCreditState.top = top;
      setPref('providers.claudeCreditTop', top);
    }
  } catch (error) {
    claudeCreditState.error = String(error);
  } finally {
    claudeCreditState.busy = false;
  }
}

export async function probeProvider(kind: ProviderKind, baseUrl: string, apiKey: string, keyEnv = ''): Promise<Probe> {
  if (!inTauri()) return { ok: false, models: [], ms: 0, error: 'Only available in the desktop app' };
  return call<Probe>('provider_probe', { kind, baseUrl, apiKey: apiKey || null, keyEnv: keyEnv || null });
}

export async function scanLocal() {
  if (!inTauri() || connectors.scanning) return;
  connectors.scanning = true;
  try {
    connectors.local = await call<LocalServer[]>('local_scan');
  } finally {
    connectors.scanning = false;
  }
}

export function saveProvider(provider: CustomProvider) {
  const index = connectors.custom.findIndex((item) => item.id === provider.id);
  if (index >= 0) connectors.custom[index] = provider;
  else connectors.custom.push(provider);
}

export function removeProvider(id: string) {
  connectors.custom = connectors.custom.filter((item) => item.id !== id);
}

export const newProviderId = () => `custom-${Date.now().toString(36)}`;

// --- Saved settings ---------------------------------------------------------------------------------------

let hydrated = false;
Promise.all([getPref<CustomProvider[]>('providers.custom'), getPref<typeof connectors.alerts>('connectors.alerts'), getPref<typeof connectors.recovery>('connectors.recovery'), getPref<ClaudeCredit>('providers.claudeCredit')]).then(([custom, alerts, recovery, credit]) => {
  if (custom) connectors.custom = custom;
  if (credit) connectors.claudeCredit = { ...connectors.claudeCredit, ...credit };
  if (recovery) connectors.recovery = { ...connectors.recovery, ...recovery };
  // Moving to another agent used to run by itself: it is off now until the user switches it on (model popover).
  if (!connectors.recovery.chosen) connectors.recovery = { ...connectors.recovery, enabled: false, chosen: true };
  if (alerts) connectors.alerts = { ...connectors.alerts, ...alerts };
  // System notifications used to start switched off, so most people never saw a finished task announced. They are on
  // now; a saved "off" from before the user ever chose (no `systemChosen`) is turned on once.
  if (!connectors.alerts.systemChosen) connectors.alerts = { ...connectors.alerts, system: true, systemChosen: true };
  hydrated = true;
  // macOS asks once; Windows and Linux grant it. Without it a notification is dropped silently.
  if (connectors.alerts.system) void requestSystemNotificationPermission();
});

$effect.root(() => {
  $effect(() => {
    const custom = $state.snapshot(connectors.custom);
    if (hydrated) setPref('providers.custom', custom);
  });
  $effect(() => {
    const alerts = { ...connectors.alerts };
    if (hydrated) setPref('connectors.alerts', alerts);
  });
  $effect(() => {
    const recovery = $state.snapshot(connectors.recovery);
    if (hydrated) setPref('connectors.recovery', recovery);
  });
  $effect(() => {
    const credit = $state.snapshot(connectors.claudeCredit);
    if (hydrated) setPref('providers.claudeCredit', credit);
  });
});
