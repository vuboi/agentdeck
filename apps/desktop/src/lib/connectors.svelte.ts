import { inTauri } from './history';
import { getPref, setPref } from './prefs';
import { toast } from './toast';

// Settings → Connectors. State and actions for: the agent CLIs (installed? signed in?), the accounts each one can
// switch between (Codex via `codex-auth`, Claude via `cswap`), the "close to the limit" alerts, and custom / local
// model providers. The work itself is in Rust (`connectors.rs`); this file holds what the screen shows.

export type ConnectorId = 'claude' | 'codex' | 'agy';
export type AccountProvider = 'claude' | 'codex';

export type Tool = { id: string; path: string | null; version: string | null };
export type Login = { logged_in: boolean | null; email: string | null; plan: string | null; detail: string | null };
export type Detected = { tools: Tool[]; claude: Login; codex: Login; antigravity: Login };
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
};
export type Accounts = {
  manager: boolean;
  accounts: Account[];
  auto_switch: { enabled: boolean; five_hour: number; weekly: number } | null;
  error: string | null;
};
export type QuotaGroup = { id: string; label: string; windows: UsageWindow[] };
export type AgyQuota = { observed_at: number | null; groups: QuotaGroup[] };
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
};
export type Probe = { ok: boolean; models: string[]; ms: number; error: string | null };
export type LocalServer = { kind: ProviderKind; label: string; base_url: string; models: string[] };

/** Codex "banked" limit resets: redeemed on ChatGPT's usage page, so this is the user's own tally. */
export type ResetLedger = { left: number; used: number; lastAt: number | null };
export const RESET_URL = 'https://chatgpt.com/codex/settings/usage';

export type Section = 'connectors' | 'providers' | 'alerts' | 'sync';

export const settings = $state({ open: false, section: 'connectors' as Section });

export function openSettings(section: Section = 'connectors') {
  settings.section = section;
  settings.open = true;
}

export const connectors = $state({
  detected: null as Detected | null,
  accounts: { claude: null, codex: null } as Record<AccountProvider, Accounts | null>,
  /** What is running right now, by key (`install:codex`, `login:claude`, `switch:codex`…), with a label for it. */
  busy: {} as Record<string, string>,
  /** Last lines of an install or sign-in, by connector id. */
  logs: {} as Record<string, string[]>,
  /** The last error of an action, by key. */
  errors: {} as Record<string, string>,
  alerts: { enabled: true, threshold: 90, system: false },
  /** Saved Codex limit resets per account email, counted by hand (OpenAI gives no way to read them). */
  resets: {} as Record<string, ResetLedger>,
  /** Email of the account whose redeemed reset is being waited for. */
  awaitingReset: '',
  custom: [] as CustomProvider[],
  local: [] as LocalServer[],
  /** What agy last reported per model family, read from the file a status-line tool saves. `null`: none saved. */
  agyQuota: null as AgyQuota | null,
  scanning: false,
});

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
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
  const { listen } = await import('@tauri-apps/api/event');
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
    connectors.detected = await call<Detected>('connectors_detect');
  });
}

let adopted = false;

export async function refreshAccounts(provider: AccountProvider, refresh = false) {
  if (!inTauri()) return;
  await task(`accounts:${provider}`, refresh ? 'Refreshing usage' : 'Reading accounts', async () => {
    const next = await call<Accounts>('accounts_list', { provider, refresh });
    // A fresh `cswap` knows no account yet: save the one Claude Code is signed in as, so the list is not empty.
    if (provider === 'claude' && next.manager && next.accounts.length === 0 && !next.error && connectors.detected?.claude.logged_in && !adopted) {
      adopted = true;
      try {
        await call('account_adopt');
        Object.assign(next, await call<Accounts>('accounts_list', { provider, refresh }));
      } catch (error) {
        next.error = `Could not save the current Claude account: ${message(error)}`;
      }
    }
    if (provider === 'codex') noticeResets(connectors.accounts.codex, next);
    connectors.accounts[provider] = next;
  });
}

export async function refreshAgyQuota() {
  if (!inTauri()) return;
  try {
    connectors.agyQuota = await call<AgyQuota | null>('antigravity_quota');
  } catch {
    connectors.agyQuota = null;
  }
}

export async function refreshAll(refresh = false) {
  await refreshDetect();
  void refreshAgyQuota();
  await Promise.all([refreshAccounts('codex', refresh), refreshAccounts('claude', refresh)]);
}

export async function install(id: string, label: string) {
  connectors.logs[id] = [];
  const ok = await task(`install:${id}`, `Installing ${label}`, () => call('connector_install', { id }));
  if (ok) {
    toast.success(`${label} installed`);
    await refreshAll();
  }
}

export async function login(id: ConnectorId, label: string) {
  connectors.logs[id] = [];
  const ok = await task(`login:${id}`, `Waiting for ${label} sign-in`, () => call('connector_login', { id }));
  if (ok) {
    if (id === 'claude') adopted = false; // a fresh sign-in may fix what stopped the account being saved
    await refreshAll();
    if (id !== 'agy') toast.success(`${label} signed in`);
  }
}

export async function addAccount(provider: AccountProvider, label: string) {
  connectors.logs[provider] = [];
  const ok = await task(`add:${provider}`, `Adding a ${label} account`, () => call('account_add', { provider }));
  if (ok) {
    toast.success(`${label} account added`);
    await refreshAll();
  }
}

export async function switchAccount(provider: AccountProvider, account: Account) {
  const ok = await task(`switch:${provider}`, `Switching to ${account.email}`, () => call('account_switch', { provider, id: account.id }));
  if (ok) {
    toast.success(`Now using ${account.email}`);
    await refreshAll();
  }
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

const announced = new Set<string>();

function announce(warning: Warning) {
  const key = `${warning.provider}|${warning.account.email}|${warning.window.label}|${warning.window.resets_at}`;
  if (announced.has(key)) return;
  announced.add(key);
  const name = providerName(warning.provider);
  const hint = warning.better ? `Switch to ${warning.better.email} in Settings.` : 'No other account has room: add one in Settings.';
  const text = `${name} ${warning.window.label} limit at ${Math.round(warning.window.pct)}% on ${warning.account.email}. ${hint}`;
  toast.error(text);
  if (connectors.alerts.system && typeof Notification !== 'undefined' && Notification.permission === 'granted') {
    try {
      new Notification(`${name} is close to its limit`, { body: text });
    } catch {
      // The toast already showed it.
    }
  }
}

export async function allowSystemNotifications(allow: boolean) {
  connectors.alerts.system = allow;
  if (allow && typeof Notification !== 'undefined' && Notification.permission === 'default') {
    const answer = await Notification.requestPermission();
    if (answer !== 'granted') connectors.alerts.system = false;
  }
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

// --- Saved limit resets (Codex) ---------------------------------------------------------------------------

/** Starts a tally for a paid Codex account the first time it is seen. 3 is what these plans start with. */
function trackResets(email: string) {
  connectors.resets[email] ??= { left: 3, used: 0, lastAt: null };
}

/**
 * Counts a reset the user redeemed on the ChatGPT page without being asked: a window that had not expired yet
 * suddenly shows far less usage. A window that simply ran out its time does not count.
 */
function noticeResets(before: Accounts | null, after: Accounts) {
  const now = Date.now();
  for (const account of after.accounts) {
    if (account.plan === 'free') continue;
    trackResets(account.email);
    const old = before?.accounts.find((item) => item.email === account.email);
    if (!old) continue;
    const dropped = account.usage.some((window) => {
      const was = old.usage.find((item) => item.label === window.label);
      return was !== undefined && (was.resets_at === null || was.resets_at * 1000 > now) && was.pct - window.pct >= 25;
    });
    const ledger = connectors.resets[account.email];
    if (dropped && ledger.left > 0) {
      ledger.left -= 1;
      ledger.used += 1;
      ledger.lastAt = now;
    }
  }
}

/**
 * After the user opens the ChatGPT page to redeem a reset: reads the usage every 15 s (for up to 5 minutes) until
 * `noticeResets` has counted it. Counting itself happens in `refreshAccounts`.
 */
export async function waitForReset(email: string) {
  const ledger = connectors.resets[email];
  if (!ledger || connectors.awaitingReset) return;
  connectors.awaitingReset = email;
  const startedWith = ledger.used;
  try {
    for (let round = 0; round < 20 && connectors.awaitingReset === email; round++) {
      await new Promise((done) => setTimeout(done, 15_000));
      await refreshAccounts('codex', true);
      if (connectors.resets[email]?.used > startedWith) {
        toast.success(`Reset counted: ${connectors.resets[email].left} left`);
        return;
      }
    }
    if (connectors.awaitingReset === email) toast.info('No reset seen yet. Use Refresh once you have redeemed it.');
  } finally {
    if (connectors.awaitingReset === email) connectors.awaitingReset = '';
  }
}

export function stopWaitingForReset() {
  connectors.awaitingReset = '';
}

// --- Custom and local providers ----------------------------------------------------------------------------

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
Promise.all([getPref<CustomProvider[]>('providers.custom'), getPref<typeof connectors.alerts>('connectors.alerts'), getPref<typeof connectors.resets>('connectors.resets')]).then(([custom, alerts, resets]) => {
  if (custom) connectors.custom = custom;
  if (resets) connectors.resets = resets;
  if (alerts) connectors.alerts = { ...connectors.alerts, ...alerts };
  hydrated = true;
});

$effect.root(() => {
  $effect(() => {
    const custom = $state.snapshot(connectors.custom);
    if (hydrated) setPref('providers.custom', custom);
  });
  $effect(() => {
    const resets = $state.snapshot(connectors.resets);
    if (hydrated) setPref('connectors.resets', resets);
  });
  $effect(() => {
    const alerts = { ...connectors.alerts };
    if (hydrated) setPref('connectors.alerts', alerts);
  });
});
