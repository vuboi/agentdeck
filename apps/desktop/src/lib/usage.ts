// Tokens a reply used and what they cost. The CLI reports tokens as it goes; its exact cost only at the end,
// so a running reply shows an estimate from the list prices below (USD per million tokens).

import { getPref, setPref } from './prefs';

export type Tokens = { input: number; output: number; cacheRead: number; cacheWrite: number };

type Price = { input: number; output: number; cacheRead: number };
const PRICES: Record<string, Price> = {
  'claude-fable-5-1': { input: 10, output: 50, cacheRead: 0.25 },
  'claude-opus-5-5': { input: 4, output: 20, cacheRead: 0.2 },
  'claude-sonnet-5-5': { input: 2, output: 10, cacheRead: 0.2 },
  'claude-haiku-5-5': { input: 0.1, output: 0.5, cacheRead: 0.01 },
};

/** Estimated cost in USD, or null when the model's price is unknown. Cache writes cost 1.25x input. */
export function estimateCost(tokens: Tokens, model: string | undefined): number | null {
  const price = model ? PRICES[model] : undefined;
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
  const price = model ? PRICES[model] : undefined;
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
  });
}

export const usageLog = async () => (await getPref<UsageRecord[]>(LOG_KEY)) ?? [];
