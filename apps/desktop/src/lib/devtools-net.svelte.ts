import { inTauri } from './history';

// What the Network tab of the developer tools shows. The page's network events come from the browser itself (the
// DevTools protocol, see src-tauri/src/cdp.rs), so every kind of request is here: documents, scripts, styles, fonts,
// images, fetch / XHR, WebSocket and event streams, with headers, timing and the response body on demand.

export type Header = { name: string; value: string };
export type Frame = { dir: 'sent' | 'received'; at: number; opcode: number; data: string };
export type NetRequest = {
  id: string;
  url: string;
  method: string;
  /** The browser's resource type: Document, Stylesheet, Script, Image, Font, Fetch, XHR, WebSocket, EventSource… */
  type: string;
  status: number;
  statusText: string;
  mime: string;
  protocol: string;
  remote: string;
  reqHeaders: Header[];
  resHeaders: Header[];
  postData: string;
  /** Epoch ms when it started; `ms` is how long it took (grows until it finishes). */
  at: number;
  ms: number;
  size: number;
  pending: boolean;
  fromCache: boolean;
  error: string;
  initiator: string;
  frames: Frame[];
  /** The response text, once asked for (`undefined`: not yet, `null`: none to show). */
  body?: string | null;
  bodyBase64?: boolean;
  startedAt: number;
};

export const net = $state({ byTab: {} as Record<string, NetRequest[]>, preserve: false });

const LIMIT = 1000;
const headersOf = (value: unknown): Header[] =>
  Object.entries((value ?? {}) as Record<string, unknown>).map(([name, item]) => ({ name, value: String(item) }));

const listOf = (tab: string) => (net.byTab[tab] ??= []);
const find = (tab: string, id: string) => listOf(tab).find((item) => item.id === id);

function blank(id: string, url: string, at: number): NetRequest {
  return {
    id, url, method: 'GET', type: 'Other', status: 0, statusText: '', mime: '', protocol: '', remote: '',
    reqHeaders: [], resHeaders: [], postData: '', at, ms: 0, size: 0, pending: true, fromCache: false, error: '',
    initiator: '', frames: [], startedAt: 0,
  };
}

type Params = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

// The app's own traffic (the page reports its console and network to the app through this host): not the page's.
const OWN = /^(https?|wss?):\/\/agentdeck\.localhost[/:]|^agentdeck:/i;
const own = new Set<string>();

function apply(tab: string, event: string, p: Params) {
  const list = listOf(tab);
  const id = String(p.requestId ?? '');
  if (event === 'Network.requestWillBeSent' && OWN.test(String(p.request?.url ?? ''))) {
    own.add(`${tab}|${id}`);
    if (own.size > 5000) own.clear();
    return;
  }
  if (own.has(`${tab}|${id}`)) return;
  switch (event) {
    case 'Network.requestWillBeSent': {
      // A new page (its document request): the list starts again, unless the log is kept.
      if (p.type === 'Document' && p.requestId === p.loaderId && !p.redirectResponse && !net.preserve) list.splice(0, list.length);
      const old = find(tab, id);
      if (old && p.redirectResponse) {
        // A redirect: the earlier hop is finished with the answer that redirected it; this one starts under the same id.
        old.status = p.redirectResponse.status;
        old.statusText = p.redirectResponse.statusText ?? '';
        old.resHeaders = headersOf(p.redirectResponse.headers);
        old.pending = false;
        old.ms = Math.max(0, Math.round((p.timestamp - old.startedAt) * 1000));
        old.id = `${id}~${list.length}`;
      }
      const request = blank(id, p.request.url, Math.round((p.wallTime ?? Date.now() / 1000) * 1000));
      request.method = p.request.method;
      request.type = p.type ?? 'Other';
      request.reqHeaders = headersOf(p.request.headers);
      request.postData = p.request.postData ?? '';
      request.startedAt = p.timestamp;
      request.initiator = p.initiator?.url ? `${p.initiator.url}${p.initiator.lineNumber !== undefined ? `:${p.initiator.lineNumber + 1}` : ''}` : (p.initiator?.type ?? '');
      list.push(request);
      if (list.length > LIMIT) list.splice(0, list.length - LIMIT);
      return;
    }
    case 'Network.responseReceived': {
      // A response whose request was not seen (it began before recording): nothing to attach it to.
      const request = find(tab, id);
      if (!request) return;
      const response = p.response;
      request.type = p.type ?? request.type;
      request.status = response.status;
      request.statusText = response.statusText ?? '';
      request.mime = response.mimeType ?? '';
      request.protocol = response.protocol ?? '';
      request.remote = response.remoteIPAddress ? `${response.remoteIPAddress}:${response.remotePort}` : '';
      request.resHeaders = headersOf(response.headers);
      request.fromCache = Boolean(response.fromDiskCache || response.fromPrefetchCache || response.fromServiceWorker);
      if (response.requestHeaders) request.reqHeaders = headersOf(response.requestHeaders);
      return;
    }
    case 'Network.requestServedFromCache': {
      const request = find(tab, id);
      if (request) request.fromCache = true;
      return;
    }
    case 'Network.loadingFinished': {
      const request = find(tab, id);
      if (!request) return;
      request.pending = false;
      request.size = p.encodedDataLength ?? 0;
      request.ms = Math.max(0, Math.round((p.timestamp - request.startedAt) * 1000));
      return;
    }
    case 'Network.loadingFailed': {
      const request = find(tab, id);
      if (!request) return;
      request.pending = false;
      request.error = p.canceled ? 'canceled' : (p.blockedReason ? `blocked: ${p.blockedReason}` : (p.errorText ?? 'failed'));
      request.ms = Math.max(0, Math.round((p.timestamp - request.startedAt) * 1000));
      return;
    }
    case 'Network.webSocketCreated': {
      const request = find(tab, id) ?? blank(id, p.url, Date.now());
      request.type = 'WebSocket';
      request.url = p.url;
      if (!find(tab, id)) list.push(request);
      return;
    }
    case 'Network.webSocketHandshakeResponseReceived': {
      const request = find(tab, id);
      if (!request) return;
      request.status = p.response.status;
      request.statusText = p.response.statusText ?? '';
      request.resHeaders = headersOf(p.response.headers);
      if (p.response.requestHeaders) request.reqHeaders = headersOf(p.response.requestHeaders);
      return;
    }
    case 'Network.webSocketFrameSent':
    case 'Network.webSocketFrameReceived': {
      const request = find(tab, id);
      if (!request) return;
      request.frames.push({ dir: event.endsWith('Sent') ? 'sent' : 'received', at: Date.now(), opcode: p.response.opcode, data: String(p.response.payloadData ?? '') });
      if (request.frames.length > 500) request.frames.shift();
      request.size += String(p.response.payloadData ?? '').length;
      return;
    }
    case 'Network.webSocketFrameError': {
      const request = find(tab, id);
      if (request) request.error = String(p.errorMessage ?? 'error');
      return;
    }
    case 'Network.webSocketClosed': {
      const request = find(tab, id);
      if (request) request.pending = false;
      return;
    }
    case 'Network.eventSourceMessageReceived': {
      const request = find(tab, id);
      if (!request) return;
      request.frames.push({ dir: 'received', at: Date.now(), opcode: 1, data: `${p.eventName ? `[${p.eventName}] ` : ''}${p.data ?? ''}` });
      if (request.frames.length > 500) request.frames.shift();
      return;
    }
  }
}

let listening = false;
export async function listenToNetwork() {
  if (listening || !inTauri()) return;
  listening = true;
  const { listen } = await import('@tauri-apps/api/event');
  await listen<{ label: string; event: string; params: Params }>('devtools-net', ({ payload }) => {
    const tab = payload.label.replace(/^browser-/, '');
    apply(tab, payload.event, payload.params);
  });
}

export function clearNetwork(tab: string) {
  net.byTab[tab] = [];
}

/** A DevTools call on a tab's page (see `devtools_cdp`): response bodies, cookies, storage, IndexedDB, caches. */
export async function cdp<T = Params>(tab: string, method: string, params: Params = {}): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>('devtools_cdp', { tab, method, params });
}

/** Loads the response text of a request (kept by the browser while it can). Images and other binaries stay base64. */
export async function loadBody(tab: string, request: NetRequest) {
  if (request.body !== undefined || request.pending) return;
  try {
    const answer = await cdp<{ body: string; base64Encoded: boolean }>(tab, 'Network.getResponseBody', { requestId: request.id.replace(/~\d+$/, '') });
    request.body = answer.body;
    request.bodyBase64 = answer.base64Encoded;
  } catch {
    request.body = null;
  }
}

// ---- display helpers shared by the tab ----

export const TYPES: { value: string; label: string; match: (type: string) => boolean }[] = [
  { value: 'all', label: 'All', match: () => true },
  { value: 'fetch', label: 'Fetch/XHR', match: (type) => type === 'Fetch' || type === 'XHR' },
  { value: 'doc', label: 'Doc', match: (type) => type === 'Document' },
  { value: 'css', label: 'CSS', match: (type) => type === 'Stylesheet' },
  { value: 'js', label: 'JS', match: (type) => type === 'Script' },
  { value: 'font', label: 'Font', match: (type) => type === 'Font' },
  { value: 'img', label: 'Img', match: (type) => type === 'Image' },
  { value: 'media', label: 'Media', match: (type) => type === 'Media' },
  { value: 'ws', label: 'WS / SSE', match: (type) => type === 'WebSocket' || type === 'EventSource' },
  { value: 'other', label: 'Other', match: (type) => !['Fetch', 'XHR', 'Document', 'Stylesheet', 'Script', 'Font', 'Image', 'Media', 'WebSocket', 'EventSource'].includes(type) },
];

export const failed = (request: NetRequest) => !request.pending && (Boolean(request.error) || request.status >= 400);

export function sizeText(bytes: number): string {
  if (bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} kB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export const timeText = (request: NetRequest) => (request.pending ? '…' : request.ms >= 1000 ? `${(request.ms / 1000).toFixed(2)} s` : `${request.ms} ms`);

/** JSON gets indented; anything else stays as it is. */
export function pretty(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}
