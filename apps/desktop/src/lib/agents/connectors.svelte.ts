import { cancelCredentialRotations, credentialBusy, credentialDelay, exclusiveCredential, waitForCredential, rotateCredential, type CredentialAttempt } from './credential-switch';
import { inTauri } from '../app/platform/history';
import { getPref, setPref } from '../app/platform/prefs';
import { loadSecret, saveSecret } from '../app/platform/secrets';
import { toast } from '../app/platform/toast';
import { accountSwitched } from '../chat/runs/account-epoch.svelte';
import { requestSystemNotificationPermission, sendSystemNotification } from '../app/platform/system-notifications';

// Settings → Connectors. State and actions for: the agent CLIs (installed? signed in?), the accounts each one can
// switch between (Codex via `codex-auth`, Claude via `cswap`), the "close to the limit" alerts, and custom / local
// model providers. The work itself is in Rust (`connectors.rs`); this file holds what the screen shows.

export type ConnectorId = 'claude' | 'codex' | 'agy' | 'opencode' | 'copilot';
export type AccountProvider = 'claude' | 'codex' | 'copilot';

export type Tool = { id: string; path: string | null; version: string | null };
export type Login = { logged_in: boolean | null; email: string | null; plan: string | null; detail: string | null };
export type Detected = { tools: Tool[]; claude: Login; codex: Login; antigravity: Login; opencode: Login; copilot: Login };
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
type SavedAccountCache = {
  savedAt: number;
  refreshedAt?: Partial<Record<ConnectorId, number>>;
  accounts: Partial<Record<AccountProvider, Accounts | null>>;
  agyAccounts: AgyAccount[];
  agyQuota: AgyQuota | null;
};
export type QuotaGroup = { id: string; label: string; windows: UsageWindow[] };
export type AgyQuota = { observed_at: number | null; groups: QuotaGroup[] };
/** One saved Antigravity account with its own quota; `error` says why there is none. */
export type AgyAccountQuota = { id: string; email: string; active: boolean; quota: AgyQuota | null; error: string | null };
export type AgyAccount = { id: string; email: string; active: boolean; expired: boolean; status: string | null };
export type ProviderKind = 'openai' | 'anthropic' | 'ollama';
export type ProviderApiKey = { id: string; label: string; apiKey: string; keyEnv?: string };
export type CustomProvider = {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  /** Kept for older provider records; normalized into the first key's keyEnv on hydration. */
  keyEnv: string;
  /** Keys stay only in this app's local IndexedDB on this computer; never in source or a build. */
  keys: ProviderApiKey[];
  activeKeyId: string | null;
  autoSwitch: boolean;
  /** Legacy single key, migrated to `keys` on hydration. */
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

export type Section = 'connectors' | 'providers' | 'replies' | 'memory' | 'office' | 'alerts' | 'sync' | 'computer' | 'debug' | 'about';

export const settings = $state({ open: false, section: 'connectors' as Section });

export function openSettings(section: Section = 'connectors') {
  settings.section = section;
  settings.open = true;
}

export const connectors = $state({
  detected: null as Detected | null,
  accounts: { claude: null, codex: null, copilot: null, agy: [] } as Record<AccountProvider, Accounts | null> & { agy: AgyAccount[] },
  autoSwitch: { claude: false, codex: false, copilot: false, agy: false, opencode: false } as Record<ConnectorId, boolean>,
  /** What is running right now, by key (`install:codex`, `login:claude`, `switch:codex`…), with a label for it. */
  busy: {} as Record<string, string>,
  /** Last lines of an install or sign-in, by connector id. */
  logs: {} as Record<string, string[]>,
  /** The last error of an action, by key. */
  errors: {} as Record<string, string>,
  alerts: { enabled: true, threshold: 90, system: true, systemChosen: false, keepAwake: true },
  /** When a turn fails: retry, or let the next agent (provider) of `chain` carry on; never another account (turn-failure.ts). */
  recovery: { enabled: false, chain: ['claude', 'codex', 'opencode', 'antigravity'] as string[], /** At a usage limit with nowhere to move, carry on by itself once the limit resets. */ autoResume: true, /** The user set `enabled` themself (it was on by default before). */ chosen: false },
  custom: [] as CustomProvider[],
  /** Claude Console API credit: Claude Code runs on this key (billed to the credit) for the `credit:` models. */
  claudeCredit: { apiKey: '', sessionKey: '', enabled: true } as ClaudeCredit,
  local: [] as LocalServer[],
  /** What agy last reported per model family, read from the file a status-line tool saves. `null`: none saved. */
  agyQuota: null as AgyQuota | null,
  agyAccountQuotas: [] as AgyAccountQuota[],
  scanning: false,
  manualRefreshing: false,
});

const ACCOUNT_CACHE_KEY = 'accounts.quota-cache';
const ACCOUNT_AUTO_SWITCH_KEY = 'accounts.auto-switch';
const accountCacheAt: Partial<Record<ConnectorId, number>> = {};
/** Account lists and quota snapshots are hydrated from IndexedDB before a live refresh replaces them. */
const accountCacheReady = getPref<SavedAccountCache>(ACCOUNT_CACHE_KEY)
  .then((saved) => {
    if (!saved) return;
    Object.assign(accountCacheAt, saved.refreshedAt ?? {});
    for (const provider of ['claude', 'codex', 'copilot'] as const) {
      const cached = saved.accounts[provider];
      if (cached) connectors.accounts[provider] = cached;
    }
    if (Array.isArray(saved.agyAccounts)) connectors.accounts.agy = saved.agyAccounts;
    if (saved.agyQuota) connectors.agyQuota = saved.agyQuota;
  })
  .catch(() => undefined);
const autoSwitchReady = getPref<Partial<Record<ConnectorId, boolean>>>(ACCOUNT_AUTO_SWITCH_KEY)
  .then((saved) => {
    if (saved) Object.assign(connectors.autoSwitch, saved);
  })
  .catch(() => undefined);

function saveAccountCache() {
  return setPref(ACCOUNT_CACHE_KEY, {
    savedAt: Date.now(),
    refreshedAt: { ...accountCacheAt },
    accounts: {
      claude: connectors.accounts.claude,
      codex: connectors.accounts.codex,
      copilot: connectors.accounts.copilot,
    },
    agyAccounts: connectors.accounts.agy,
    agyQuota: connectors.agyQuota,
  } satisfies SavedAccountCache);
}

const activeLoginRuns = new Map<ConnectorId, number>();
const cancelledLoginRuns = new Set<number>();
const activeAccountAddRuns = new Map<AccountProvider, number>();
const cancelledAccountAddRuns = new Set<number>();
let loginSequence = 0;

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
};

type StoredCustomProvider = Omit<Partial<CustomProvider>, 'id' | 'name' | 'kind' | 'baseUrl' | 'keyEnv' | 'models' | 'defaultModel' | 'enabled'> &
  Pick<CustomProvider, 'id' | 'name' | 'kind' | 'baseUrl' | 'models' | 'defaultModel' | 'enabled'> & { keyEnv?: string };

/** Converts the old single-key provider shape to a stable key list without changing provider identity or settings. */
export function normalizeCustomProvider(provider: StoredCustomProvider): CustomProvider {
  const keys = Array.isArray(provider.keys)
    ? provider.keys.map((item, index) => ({
        id: item.id || `${provider.id}-key-${index + 1}`,
        label: item.label?.trim() || `Key ${index + 1}`,
        apiKey: item.apiKey ?? '',
        ...(item.keyEnv ? { keyEnv: item.keyEnv } : {}),
      }))
    : [{
        id: `${provider.id}-key-1`,
        label: 'Key 1',
        apiKey: provider.apiKey ?? '',
        ...(provider.keyEnv ? { keyEnv: provider.keyEnv } : {}),
      }];
  const activeKeyId = keys.some((item) => item.id === provider.activeKeyId) ? provider.activeKeyId! : keys[0]?.id ?? null;
  const { apiKey: _legacyApiKey, ...saved } = provider;
  return {
    ...saved,
    keyEnv: provider.keyEnv ?? '',
    keys,
    activeKeyId,
    autoSwitch: Boolean(provider.autoSwitch),
  } as CustomProvider;
}

export function activeProviderKey(provider: CustomProvider): ProviderApiKey | undefined {
  return provider.keys.find((key) => key.id === provider.activeKeyId) ?? provider.keys[0];
}

export function providerKeyEnv(provider: CustomProvider, key = activeProviderKey(provider)) {
  return key?.keyEnv || provider.keyEnv || '';
}

/** Captures identity only, never a secret, for the credential that actually starts a turn. */
export function currentCredentialId(agent: string, providerId?: string): string | undefined {
  if (agent === 'custom') return connectors.custom.find((item) => item.id === providerId)?.activeKeyId ?? undefined;
  if (agent === 'antigravity') return connectors.accounts.agy.find((item) => item.active)?.id;
  if (agent === 'claude' || agent === 'codex' || agent === 'copilot') return connectors.accounts[agent]?.accounts.find((item) => item.active)?.id;
  return undefined;
}

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

/** Reloads the account-specific Copilot model list after GitHub sign-in or account switching. */
function refreshCopilotModels() {
  void import('./models.svelte').then(({ loadModels }) => loadModels(['copilot']));
}
export const installed = (id: string) => Boolean(tool(id)?.path);

let listening = false;
async function listenToLogs() {
  if (listening || !inTauri()) return;
  listening = true;
  const { listen } = await import('../app/platform/tauri-event');
  await listen<{ id: string; line: string }>('connector-log', ({ payload }) => {
    const lines = (connectors.logs[payload.id] ??= []);
    lines.push(payload.line);
    if (lines.length > 60) lines.splice(0, lines.length - 60);
    openSignInPage(payload.id, payload.line);
  });
}

// Claude, Codex and Antigravity open the browser themselves. OpenCode and `gh auth login --web` run here without a terminal,
// so they only print the address: the first one printed during a sign-in is opened for them (once per sign-in).
const OPENS_ITSELF_NOT = new Set<string>(['opencode', 'copilot']);
const signInPageOpened = new Set<string>();
function openSignInPage(id: string, line: string) {
  if (!OPENS_ITSELF_NOT.has(id) || !activeLoginRuns.has(id as ConnectorId) || signInPageOpened.has(id)) return;
  const url = line.match(/https?:\/\/[^\s"'<>]+/)?.[0]?.replace(/[.,;:)]+$/, '');
  if (!url) return;
  signInPageOpened.add(id);
  void import('../browser/browser.svelte').then(({ openExternal }) => openExternal(url));
}

export async function refreshDetect() {
  if (!inTauri()) return;
  void listenToLogs();
  await task('detect', 'Checking', async () => {
    connectors.detected = await callWithTimeout<Detected>('connectors_detect', undefined, 30_000);
  });
}

let adopted = false;

export async function refreshAccounts(provider: AccountProvider, refresh = false, duringSwitch = false) {
  if (!duringSwitch && credentialBusy(provider)) await waitForCredential(provider);
  if (!inTauri()) return;
  await Promise.all([accountCacheReady, autoSwitchReady]);
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
    accountCacheAt[provider] = Date.now();
    void saveAccountCache();
  });
}

export async function refreshAgyQuota(duringSwitch = false) {
  if (!duringSwitch && credentialBusy('agy')) await waitForCredential('agy');
  if (!inTauri()) return;
  await accountCacheReady;
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
    if (live) {
      connectors.agyQuota = live;
      void saveAccountCache();
    }
  } catch {
    // Preserve saved figures when the live quota endpoint is unavailable.
  }
  try {
    connectors.agyAccountQuotas = await call<AgyAccountQuota[]>('antigravity_quotas');
  } catch {
    // Keep the last figures of each account.
  }
}

let agyAccountsRefresh: Promise<boolean> | undefined;

export async function refreshAgyAccounts(duringSwitch = false) {
  if (!duringSwitch && credentialBusy('agy')) await waitForCredential('agy');
  if (!inTauri()) return;
  await accountCacheReady;
  if (agyAccountsRefresh) return agyAccountsRefresh;
  const refresh = task('accounts:agy', 'Reading Antigravity accounts', async () => {
    connectors.accounts.agy = await call<AgyAccount[]>('antigravity_accounts_list');
    accountCacheAt.agy = Date.now();
    void saveAccountCache();
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
    accountCacheAt.agy = Date.now();
  });
  if (ok) {
    delete connectors.errors['login:agy'];
    toast.success('Antigravity account saved');
  }
  return ok;
}

export async function switchAgyAccount(account: AgyAccount): Promise<boolean> {
  cancelCredentialRotations('agy');
  const ok = await task('switch:agy', `Switching to ${account.email}`, () => exclusiveCredential('agy', () => call('antigravity_account_switch', { id: account.id })));
  if (ok) {
    accountSwitched('antigravity');
    toast.success(`Now using ${account.email}`);
    await Promise.all([refreshAgyAccounts(), refreshDetect(), refreshAgyQuota()]);
  }
  return ok;
}

export async function refreshAll(refresh = false) {
  if (refresh && connectors.manualRefreshing) return;
  await Promise.all([accountCacheReady, autoSwitchReady]);
  if (refresh) connectors.manualRefreshing = true;
  try {
    // Keep Claude detection first because account adoption depends on its signed-in state.
    // Codex and Antigravity account reads can proceed while CLI detection runs.
    void refreshAgyQuota();
    await Promise.all([refreshDetect(), refreshAccounts('codex', refresh), refreshAccounts('copilot'), refreshAgyAccounts()]);
    await refreshAccounts('claude', refresh);
  } finally {
    if (refresh) connectors.manualRefreshing = false;
  }
}

export async function install(id: string, label: string) {
  connectors.logs[id] = [];
  // The busy state stays until the new CLI is detected, so the card does not flash back to "Install" in between.
  const ok = await task(`install:${id}`, `Installing ${label}`, async () => {
    await call('connector_install', { id });
    await refreshDetect();
  });
  if (ok) {
    toast.success(`${label} installed`);
    await refreshAll();
  }
}

export async function login(id: ConnectorId, label: string): Promise<boolean> {
  if (connectors.busy[`login:${id}`]) return false;
  const run = ++loginSequence;
  activeLoginRuns.set(id, run);
  signInPageOpened.delete(id);
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
    if (id === 'copilot') refreshCopilotModels();
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
    if (provider === 'copilot') refreshCopilotModels();
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
  cancelCredentialRotations(provider);
  const ok = await task(`switch:${provider}`, `Switching to ${account.email}`, () => exclusiveCredential(provider, () => call('account_switch', { provider, id: account.id })));
  if (ok) {
    accountSwitched(provider);
    toast.success(`Now using ${account.email}`);
    await refreshAll();
    if (provider === 'copilot') refreshCopilotModels();
  }
  return ok;
}

export async function removeAccount(provider: AccountProvider, account: Account) {
  // One key per account so several can be removed at once.
  const ok = await task(`remove:${provider}:${account.id}`, 'Removing', () => call('account_remove', { provider, id: account.id }));
  if (ok) {
    toast.success(`${account.email} removed`);
    await refreshAll();
  }
}

export async function setCodexAutoSwitch(enabled: boolean, fiveHour: number, weekly: number) {
  const ok = await task('auto:codex', 'Saving', () => call('codex_auto_switch', { enabled, fiveHour, weekly }));
  if (ok) await refreshAccounts('codex');
}

/** Persists the independent automatic account-switch preference for one connector. */
export async function setConnectorAutoSwitch(connector: ConnectorId, enabled: boolean) {
  await autoSwitchReady;
  if (!enabled) cancelCredentialRotations(connector);
  connectors.autoSwitch[connector] = enabled;
  void setPref(ACCOUNT_AUTO_SWITCH_KEY, { ...connectors.autoSwitch });
}

/** Tries eligible connector profiles in quota order. Each candidate is only committed after a live usage read succeeds. */
export async function autoSwitchConnectorAccount(connector: ConnectorId, attempt: CredentialAttempt): Promise<{ email: string } | null> {
  return rotateCredential(connector, attempt, (signal) => switchConnectorCandidate(connector, attempt, signal));
}

/** Restores the original profile and confirms native state before another attempt can start. */
async function restoreConnectorAccount(connector: ConnectorId, id: string | undefined): Promise<void> {
  if (!id || connector === 'opencode') throw new Error('No original account available to restore');
  if (connector === 'agy') {
    const before = connectors.accounts.agy.find((item) => item.active)?.id;
    try { await call('antigravity_account_switch', { id }); }
    catch { /* Confirm the actual native profile below even if the command reported an error. */ }
    finally { connectors.accounts.agy = await callWithTimeout<AgyAccount[]>('antigravity_accounts_list', undefined, 65_000); void saveAccountCache(); }
    const active = connectors.accounts.agy.find((item) => item.active)?.id;
    if (active !== before) accountSwitched('antigravity');
    if (active !== id) throw new Error('Could not restore original account');
  } else {
    const before = connectors.accounts[connector]?.accounts.find((item) => item.active)?.id;
    try { await call('account_switch', { provider: connector, id }); }
    catch { /* The fresh account list below decides whether restoration succeeded. */ }
    finally {
      connectors.accounts[connector] = await callWithTimeout<Accounts>('accounts_list', { provider: connector, refresh: true }, 65_000);
      void saveAccountCache();
    }
    const active = connectors.accounts[connector]?.accounts.find((item) => item.active)?.id;
    if (active !== before) accountSwitched(connector);
    if (active !== id) throw new Error('Could not restore original account');
  }
  void refreshDetect();
}

async function switchConnectorCandidate(connector: ConnectorId, attempt: CredentialAttempt, signal: AbortSignal): Promise<{ email: string } | null> {
  await Promise.all([accountCacheReady, autoSwitchReady]);
  if (!connectors.autoSwitch[connector] || !inTauri()) return null;
  signal.throwIfAborted();
  if (connector === 'agy') {
    const alreadySelected = connectors.accounts.agy.find((item) => item.active && !item.expired);
    if (attempt.failedId && alreadySelected && alreadySelected.id !== attempt.failedId && !attempt.tried.has(alreadySelected.id)) return { email: alreadySelected.email };

    if (Date.now() - (accountCacheAt.agy ?? 0) > 60_000) await Promise.all([refreshAgyAccounts(true), refreshAgyQuota(true)]);
    let accounts = connectors.accounts.agy.filter((item) => !item.active && !item.expired);
    if (accounts.length === 0) {
      await Promise.all([refreshAgyAccounts(true), refreshAgyQuota(true)]);
      accounts = connectors.accounts.agy.filter((item) => !item.active && !item.expired);
    }
    attempt.originalId ??= connectors.accounts.agy.find((item) => item.active)?.id;
    const original = attempt.originalId;
    if (!original) return null;
    const failed = connectors.accounts.agy.find((item) => item.active)?.id;
    if (failed) attempt.tried.add(failed);
    for (const account of accounts) {
      if (attempt.tried.has(account.id)) continue;
      await credentialDelay(signal);
      attempt.tried.add(account.id);
      try {
        await call('antigravity_account_switch', { id: account.id });
        const quota = await call<AgyQuota | null>('antigravity_quota');
        signal.throwIfAborted();
        const family = attempt.model.startsWith('gemini-') ? 'gemini' : '3p';
        const windows = quota?.groups.find((group) => group.id === family)?.windows;
        if (!windows?.length || windows.some((window) => liveWindow(window, Date.now()) && window.pct >= 98)) throw new Error('No usable model quota');
        const verifiedAccounts = await call<AgyAccount[]>('antigravity_accounts_list');
        signal.throwIfAborted();
        if (verifiedAccounts.some((item) => item.id === account.id && item.active && !item.expired)) {
          connectors.agyQuota = quota;
          connectors.accounts.agy = verifiedAccounts;
          accountSwitched('antigravity');
          void saveAccountCache();
          void refreshDetect();
          return { email: account.email };
        }
        throw new Error('Account check failed');
      } catch {
        await restoreConnectorAccount(connector, original);
        signal.throwIfAborted();
      }
    }
    await restoreConnectorAccount(connector, original);
    return null;
  }
  if (connector === 'opencode') return null;
  const provider = connector as AccountProvider;
  if (Date.now() - (accountCacheAt[provider] ?? 0) > 60_000) await refreshAccounts(provider, true, true);
  const current = connectors.accounts[provider];
  if (!current) return null;
  const alreadySelected = current.accounts.find((item) => item.active && hasRoom(item, Date.now()));
  if (attempt.failedId && alreadySelected && alreadySelected.id !== attempt.failedId && !attempt.tried.has(alreadySelected.id)) return { email: alreadySelected.email };
  const now = Date.now();
  const rank = (accounts: Account[]) => accounts.filter((account) => !account.active && !account.expired && hasRoom(account, Date.now()) && !attempt.tried.has(account.id)).sort((a, b) => {
    const remaining = (account: Account) => account.usage.reduce((min, window) => !liveWindow(window, now) ? min : Math.min(min, 100 - window.pct), 100);
    return Number(b.usage.length > 0) - Number(a.usage.length > 0) || remaining(b) - remaining(a);
  });
  let candidates = rank(current.accounts);
  if (candidates.length === 0) {
    await refreshAccounts(provider, true, true);
    candidates = rank(connectors.accounts[provider]?.accounts ?? []);
  }
  attempt.originalId ??= current.accounts.find((account) => account.active)?.id;
  const original = attempt.originalId;
  if (!original) return null;
  const failed = current.accounts.find((account) => account.active)?.id;
  if (failed) attempt.tried.add(failed);
  for (const account of candidates) {
    await credentialDelay(signal);
    attempt.tried.add(account.id);
    try {
      await call('account_switch', { provider, id: account.id });
      const verified = await callWithTimeout<Accounts>('accounts_list', { provider, refresh: true }, 65_000);
      signal.throwIfAborted();
      // Managers may retain old account/quota rows alongside a failed live connection check.
      if (verified.error) throw new Error('Live account verification failed');
      if (verified.accounts.some((item) => item.id === account.id && item.active && !item.expired && hasRoom(item, Date.now()))) {
        connectors.accounts[provider] = verified;
        accountSwitched(provider);
        void saveAccountCache();
        void refreshDetect();
        if (provider === 'copilot') void refreshCopilotModels();
        return { email: account.email };
      }
      throw new Error('Account check failed');
    } catch {
      await restoreConnectorAccount(connector, original);
      signal.throwIfAborted();
    }
  }
  await restoreConnectorAccount(connector, original);
  return null;
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

const NAMES: Record<AccountProvider, string> = { claude: 'Claude', codex: 'Codex', copilot: 'GitHub Copilot' };
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

/** Asks the system to stay awake (or not) while the app runs: a long agent task is cut off when the computer sleeps. */
export async function setKeepAwake(on: boolean) {
  connectors.alerts.keepAwake = on;
  if (!inTauri()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('keep_awake_set', { on });
  } catch {
    // A system without the tool keeps its own sleep rules.
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
    await Promise.all([...targets.map((provider) => refreshAccounts(provider, true)), refreshAccounts('copilot', true)]);
    for (const warning of warnings()) announce(warning);
  };
  setTimeout(() => void check(), 20_000);
  setInterval(() => void check(), 5 * 60_000);
}

// --- Custom and local providers ----------------------------------------------------------------------------

/** What a provider says about the spending left on a key (OpenRouter), by provider id. */
export type ProviderQuota = { used: number; limit: number | null; remaining: number | null; free_tier: boolean; at: number };
export const providerQuotas = $state<Record<string, ProviderQuota>>({});
const providerQuotaCacheReady = getPref<Record<string, ProviderQuota>>('providers.quota-cache').then((saved) => {
  if (saved) Object.assign(providerQuotas, saved);
}).catch(() => undefined);

/** Reads the spending left on a custom provider's key, when the provider tells it; leaves it unset otherwise. */
export async function refreshProviderQuota(provider: CustomProvider) {
  if (!inTauri()) return;
  await providerQuotaCacheReady;
  const key = activeProviderKey(provider);
  try {
    const found = await call<Omit<ProviderQuota, 'at'> | null>('provider_quota', { baseUrl: provider.baseUrl, apiKey: key?.apiKey || null, keyEnv: providerKeyEnv(provider, key) || null });
    const cacheKey = `${provider.id}:${key?.id ?? 'default'}`;
    if (found) providerQuotas[cacheKey] = { ...found, at: Date.now() / 1000 };
    else delete providerQuotas[cacheKey];
    void setPref('providers.quota-cache', { ...providerQuotas });
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

/** Finds and verifies another API key, preferring cached keys with the most remaining quota. */
export async function autoSwitchProviderKey(provider: CustomProvider, attempt: CredentialAttempt): Promise<{ label: string } | null> {
  return rotateCredential(provider.id, attempt, (signal) => {
    const current = connectors.custom.find((item) => item.id === provider.id);
    return current ? switchProviderCandidate(current, attempt, signal) : Promise.resolve(null);
  });
}

async function switchProviderCandidate(provider: CustomProvider, attempt: CredentialAttempt, signal: AbortSignal): Promise<{ label: string } | null> {
  if (!provider.autoSwitch || provider.keys.length < 2) return null;
  await providerQuotaCacheReady;
  signal.throwIfAborted();
  const currentId = provider.activeKeyId;
  const alreadySelected = activeProviderKey(provider);
  if (attempt.failedId && alreadySelected && currentId !== attempt.failedId && !attempt.tried.has(alreadySelected.id)) return { label: alreadySelected.label };
  attempt.originalId ??= currentId ?? undefined;
  if (currentId) attempt.tried.add(currentId);
  const keys = provider.keys.filter((item) => item.id !== currentId && !attempt.tried.has(item.id)).sort((a, b) => {
    const qa = providerQuotas[`${provider.id}:${a.id}`]?.remaining;
    const qb = providerQuotas[`${provider.id}:${b.id}`]?.remaining;
    return (qb ?? -1) - (qa ?? -1);
  });
  for (const key of keys) {
    await credentialDelay(signal);
    attempt.tried.add(key.id);
    const cacheKey = `${provider.id}:${key.id}`;
    let quota = providerQuotas[cacheKey];
    if (!quota || Date.now() / 1000 - quota.at > 60) {
      try {
        const found = await call<Omit<ProviderQuota, 'at'> | null>('provider_quota', { baseUrl: provider.baseUrl, apiKey: key.apiKey || null, keyEnv: key.keyEnv || provider.keyEnv || null });
        if (found) {
          quota = { ...found, at: Date.now() / 1000 };
          providerQuotas[cacheKey] = quota;
          void setPref('providers.quota-cache', { ...providerQuotas });
        }
      } catch {
        // Not all provider endpoints expose key quota; the connection probe remains authoritative for auth.
      }
    }
    if (quota?.remaining !== null && quota?.remaining !== undefined && quota.remaining <= 0) continue;
    signal.throwIfAborted();
    const result = await probeProvider(provider.kind, provider.baseUrl, key.apiKey, key.keyEnv ?? provider.keyEnv).catch(() => null);
    signal.throwIfAborted();
    if (!provider.autoSwitch) return null;
    if (!connectors.custom.some((item) => item.id === provider.id && item.keys.some((saved) => saved.id === key.id && saved.apiKey === key.apiKey))) continue;
    if (result?.ok && (!result.models.length || result.models.includes(attempt.model))) {
      provider.activeKeyId = key.id;
      saveProvider($state.snapshot(provider) as CustomProvider, true);
      return { label: key.label || `••••${key.apiKey.slice(-4)}` };
    }
  }
  if (provider.keys.some((key) => key.id === attempt.originalId)) provider.activeKeyId = attempt.originalId!;
  return null;
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

export function saveProvider(provider: CustomProvider, automatic = false) {
  if (!automatic) cancelCredentialRotations(provider.id);
  provider = normalizeCustomProvider(provider);
  const index = connectors.custom.findIndex((item) => item.id === provider.id);
  if (index >= 0) connectors.custom[index] = provider;
  else connectors.custom.push(provider);
}

export function removeProvider(id: string) {
  cancelCredentialRotations(id);
  const removed = connectors.custom.find((item) => item.id === id);
  for (const key of removed?.keys ?? []) forgetSecret(`provider-key:${key.id}`);
  // A Claude Console key removed from the list is no longer the credit key either (the Claude card shows it as not added).
  if (removed?.kind === 'anthropic' && connectors.claudeCredit.apiKey && removed.keys.some((entry) => entry.apiKey === connectors.claudeCredit.apiKey)) {
    connectors.claudeCredit = { ...connectors.claudeCredit, apiKey: '' };
  }
  connectors.custom = connectors.custom.filter((item) => item.id !== id);
}

export const newProviderId = () => `custom-${Date.now().toString(36)}`;

// --- Saved settings ---------------------------------------------------------------------------------------

// Keys saved to the system store this session, by name: one is written again only when it changed.
const savedSecrets = new Map<string, string>();

/** What the database keeps for a key: nothing when the system store holds it, the value itself when the store refused it. */
async function keepSecret(name: string, value: string): Promise<string> {
  if (!value) {
    if (savedSecrets.delete(name)) await saveSecret(name, '');
    return '';
  }
  if (savedSecrets.get(name) === value) return '';
  if (await saveSecret(name, value)) {
    savedSecrets.set(name, value);
    return '';
  }
  return value;
}

/** A key's value: the database copy when it still has one (an older save, or the store was unavailable), else the system store. */
async function restoreSecret(name: string, stored: string | undefined): Promise<string> {
  if (stored) return stored;
  const value = (await loadSecret(name)) ?? '';
  if (value) savedSecrets.set(name, value);
  return value;
}

/** Removes a key from the system store when its provider is removed. */
function forgetSecret(name: string) {
  savedSecrets.delete(name);
  void saveSecret(name, '');
}

async function persistCustom(custom: CustomProvider[]) {
  const stored = await Promise.all(
    custom.map(async (provider) => ({
      ...provider,
      apiKey: undefined,
      keys: await Promise.all(provider.keys.map(async (key) => ({ ...key, apiKey: await keepSecret(`provider-key:${key.id}`, key.apiKey) }))),
    })),
  );
  void setPref('providers.custom', stored);
}

async function persistClaudeCredit(credit: ClaudeCredit) {
  void setPref('providers.claudeCredit', {
    ...credit,
    apiKey: await keepSecret('claude-credit:apiKey', credit.apiKey),
    sessionKey: await keepSecret('claude-credit:sessionKey', credit.sessionKey),
  });
}

/** The providers and the Console credit with their keys, for an export that includes keys (the database copy has none). */
export function providerKeysForExport() {
  return { custom: $state.snapshot(connectors.custom) as CustomProvider[], claudeCredit: $state.snapshot(connectors.claudeCredit) as ClaudeCredit };
}

let hydrated = false;
Promise.all([getPref<StoredCustomProvider[]>('providers.custom'), getPref<typeof connectors.alerts>('connectors.alerts'), getPref<typeof connectors.recovery>('connectors.recovery'), getPref<ClaudeCredit>('providers.claudeCredit')]).then(async ([custom, alerts, recovery, credit]) => {
  if (custom) {
    connectors.custom = await Promise.all(
      custom.map(async (provider) =>
        normalizeCustomProvider({
          ...provider,
          keys: await Promise.all(
            (provider.keys ?? []).map(async (key, index) => {
              const id = key.id || `${provider.id}-key-${index + 1}`;
              return { ...key, id, apiKey: await restoreSecret(`provider-key:${id}`, key.apiKey) };
            }),
          ),
        }),
      ),
    );
  }
  if (credit) {
    connectors.claudeCredit = {
      ...connectors.claudeCredit,
      ...credit,
      apiKey: await restoreSecret('claude-credit:apiKey', credit.apiKey),
      sessionKey: await restoreSecret('claude-credit:sessionKey', credit.sessionKey),
    };
  }
  if (recovery) connectors.recovery = { ...connectors.recovery, ...recovery };
  // Moving to another agent used to run by itself: it is off now until the user switches it on (model popover).
  if (!connectors.recovery.chosen) connectors.recovery = { ...connectors.recovery, enabled: false, chosen: true };
  if (alerts) connectors.alerts = { ...connectors.alerts, ...alerts };
  // System notifications used to start switched off, so most people never saw a finished task announced. They are on
  // now; a saved "off" from before the user ever chose (no `systemChosen`) is turned on once.
  if (!connectors.alerts.systemChosen) connectors.alerts = { ...connectors.alerts, system: true, systemChosen: true };
  hydrated = true;
  // On by default, and kept on: asked again at every start (the system forgets it when the app ends).
  void setKeepAwake(connectors.alerts.keepAwake !== false);
  // macOS asks once; Windows and Linux grant it. Without it a notification is dropped silently.
  if (connectors.alerts.system) void requestSystemNotificationPermission();
});

$effect.root(() => {
  $effect(() => {
    const custom = $state.snapshot(connectors.custom) as CustomProvider[];
    if (hydrated) void persistCustom(custom);
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
    const credit = $state.snapshot(connectors.claudeCredit) as ClaudeCredit;
    if (hydrated) void persistClaudeCredit(credit);
  });
});
