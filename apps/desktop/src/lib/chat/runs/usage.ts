// Tokens a reply used and what they cost. The CLI reports tokens as it goes; its exact cost only at the end,
// so a running reply shows an estimate from the list prices below (USD per million tokens).

import { getPref, setPref } from '../../app/platform/prefs';
import { baseModel } from '../../agents/credit-model';

export type Tokens = { input: number; output: number; cacheRead: number; cacheWrite: number };

type Price = { input: number; output: number; cacheRead: number };
const PRICES: Record<string, Price> = {
  'claude-fable-5-1': { input: 10, output: 50, cacheRead: 0.25 },
  'claude-opus-5-5': { input: 4, output: 20, cacheRead: 0.2 },
  'claude-sonnet-5-5': { input: 2, output: 10, cacheRead: 0.2 },
  'claude-haiku-5-5': { input: 0.1, output: 0.5, cacheRead: 0.01 },
};

// Prices the user typed for models that have none above (Codex, Gemini, local ones...): kept in prefs, read once at
// start, and used for every reply, past ones included (the dashboard works a cost out again when the reply had none).
const PRICES_KEY = 'usage.prices';
const custom: Record<string, Price> = {};
const priceListeners = new Set<() => void>();
if (typeof window !== 'undefined') {
  void getPref<Record<string, Price>>(PRICES_KEY).then((saved) => {
    if (saved) Object.assign(custom, saved);
    for (const listener of priceListeners) listener();
  });
}
/** Calls `listener` when a price is set or loaded; returns how to stop. */
export function onPrices(listener: () => void) {
  priceListeners.add(listener);
  return () => void priceListeners.delete(listener);
}
/** The price per million tokens of a model: the built-in one, else the one the user typed. */
// List prices (USD per million tokens) of models that report no cost, from the providers' published rates in October
// 2026, matched by family so an id with a prefix (`opencode/…`) or a point release (`gpt-6.1-sol`) is found. Cached
// input is billed at a tenth of the input rate where the provider gives no figure. These are estimates: a subscription
// is not charged this way, and rates change (Gemini 4 Argon's introductory rate ends on 30 November 2026).
const LIST: [RegExp, () => Price][] = [
  [/gpt-6(?:\.\d+)?-sol/i, () => ({ input: 2, output: 10, cacheRead: 0.2 })],
  [/gpt-6(?:\.\d+)?-luna/i, () => ({ input: 0.1, output: 0.5, cacheRead: 0.01 })],
  [/gpt-5\.6-sol/i, () => ({ input: 4, output: 20, cacheRead: 0.4 })],
  [/gpt-5\.6-luna/i, () => ({ input: 0.2, output: 1.2, cacheRead: 0.02 })],
  [/gemini-4-argon/i, () => (Date.now() >= Date.UTC(2026, 11, 1) ? { input: 4, output: 20, cacheRead: 0.4 } : { input: 2, output: 10, cacheRead: 0.2 })],
];
/** A free model (OpenCode's `…-free`, a provider's free tier): it costs nothing, so its replies count as $0. */
export const isFreeModel = (model: string | undefined) => Boolean(model && /(?:^|[-_:/ ])free(?:$|[-_:/ ])/i.test(model));
const FREE: Price = { input: 0, output: 0, cacheRead: 0 };
/** A model with no list price, estimated from the tier its name says: the small, fast ones (flash, mini, lite…) at the
 *  small-model rate. Only an estimate (shown with ≈), and a typed price wins over it. */
const SMALL_TIER = /(?:flash|mini|lite|nano|haiku|luna|small)/i;
const SMALL_PRICE: Price = { input: 0.1, output: 0.5, cacheRead: 0.01 };
/** Any other model without a list price: a mid-size rate, so every reply has a cost estimate (shown with ≈). */
const MID_PRICE: Price = { input: 1, output: 5, cacheRead: 0.1 };
const listed = (model: string): Price =>
  isFreeModel(model) ? FREE : (LIST.find(([pattern]) => pattern.test(model))?.[1]() ?? (SMALL_TIER.test(model) ? SMALL_PRICE : MID_PRICE));
// A `credit:` model is the same model billed to the API credit: it costs what that model costs.
export const priceOf = (model: string | undefined): Price | undefined => {
  if (!model) return undefined;
  const base = baseModel(model);
  return custom[model] ?? custom[base] ?? PRICES[base] ?? listed(base);
};
/** Saves a price for a model (`null` removes it). */
export function setModelPrice(model: string, price: Price | null) {
  if (price) custom[model] = price;
  else delete custom[model];
  setPref(PRICES_KEY, { ...custom });
  for (const listener of priceListeners) listener();
}
export type { Price };

/** Estimated cost in USD, or null when the model's price is unknown. Cache writes cost 1.25x input. */
export function estimateCost(tokens: Tokens, model: string | undefined): number | null {
  const price = priceOf(model);
  if (!price) return null;
  const perM = (count: number, rate: number) => (count / 1_000_000) * rate;
  return perM(tokens.input, price.input) + perM(tokens.cacheWrite, price.input * 1.25) + perM(tokens.cacheRead, price.cacheRead) + perM(tokens.output, price.output);
}

/** 950, 12.3k, 1.2M */
export function shortCount(count: number): string {
  if (count < 1000) return String(count);
  if (count < 1_000_000) return `${(count / 1000).toFixed(count < 10_000 ? 1 : 0)}k`;
  return `${(count / 1_000_000).toFixed(1)}M`;
}

export const usd = (value: number) => (value < 0.01 ? `$${value.toFixed(3)}` : `$${value.toFixed(2)}`);

/** All tokens the reply moved: what it read (fresh and cached) and what it wrote. */
export const totalTokens = (tokens: Tokens) => tokens.input + tokens.cacheRead + tokens.cacheWrite + tokens.output;

/** What each kind of token cost, in USD (null when the model's price is unknown). */
export function costParts(tokens: Tokens, model: string | undefined) {
  const price = priceOf(model);
  if (!price) return null;
  const perM = (count: number, rate: number) => (count / 1_000_000) * rate;
  return {
    input: perM(tokens.input, price.input),
    cacheRead: perM(tokens.cacheRead, price.cacheRead),
    cacheWrite: perM(tokens.cacheWrite, price.input * 1.25),
    output: perM(tokens.output, price.output),
  };
}

/** One finished reply, as kept for the usage dashboard. `cost` is the CLI's exact figure when it gave one. */
export type UsageRecord = {
  at: string;
  sessionId: string;
  agent: string;
  model?: string;
  projectId?: string;
  ms?: number;
  tokens: Tokens;
  cost: number | null;
  estimated: boolean;
};

const LOG_KEY = 'usage.log';
const listeners = new Set<() => void>();
/** Calls `listener` whenever a reply is added to the log; returns how to stop. */
export function onUsage(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}
const LOG_LIMIT = 20000;
let writing: Promise<void> = Promise.resolve();

/** Appends a reply to the usage log (IndexedDB), one write at a time. */
export function recordUsage(record: UsageRecord) {
  writing = writing.then(async () => {
    const log = (await getPref<UsageRecord[]>(LOG_KEY)) ?? [];
    log.push(record);
    setPref(LOG_KEY, log.length > LOG_LIMIT ? log.slice(-LOG_LIMIT) : log);
    for (const listener of listeners) listener();
    // One failed write (or a listener that throws) must not leave the chain rejected: every later record was skipped.
  }).catch(() => undefined);
}

export const usageLog = async () => (await getPref<UsageRecord[]>(LOG_KEY)) ?? [];
