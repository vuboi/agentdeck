import type { Rect } from '../../browser/browser.svelte';

// Pure helpers of the browser pane: address parsing, the lock's site info, and cropping a captured page.

export const SEARCH = 'https://www.google.com/search?q=';
export const searchUrl = (text: string) => `${SEARCH}${encodeURIComponent(text)}`;
export type Suggestion = { kind: 'go' | 'search' | 'history' | 'google'; text: string; url: string; title?: string };

export function normalize(input: string): string {
  const text = input.trim();
  if (!text) return '';
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) return text;
  if (/^(localhost|127\.|\[::1\])/.test(text)) return `http://${text}`;
  // Words with spaces are a search, not an address.
  if (/\s/.test(text) || !/\./.test(text)) return `https://www.google.com/search?q=${encodeURIComponent(text)}`;
  return `https://${text}`;
}

export type SiteSecurity = { kind: 'secure' | 'local' | 'insecure'; protocol: string; host: string; port: string };

export function siteSecurity(address: string): SiteSecurity | null {
  try {
    const url = new URL(address);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
    const local = /^(localhost|127\.0\.0\.1|\[::1\])$/.test(url.hostname) || url.hostname.endsWith('.localhost');
    const secure = url.protocol === 'https:';
    return {
      kind: secure ? ('secure' as const) : local ? ('local' as const) : ('insecure' as const),
      protocol: url.protocol.slice(0, -1).toUpperCase(),
      host: url.hostname,
      port: url.port || (secure ? '443' : '80'),
    };
  } catch {
    return null;
  }
}

export const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max)}…` : text);

export async function cropScreenshot(dataUrl: string, box: Rect, viewport: { w: number; h: number }): Promise<string> {
  const image = new Image();
  image.src = dataUrl;
  await image.decode();
  const scale = image.naturalWidth / viewport.w;
  const x = Math.max(0, Math.round(box.x * scale));
  const y = Math.max(0, Math.round(box.y * scale));
  const width = Math.min(image.naturalWidth - x, Math.round(box.w * scale));
  const height = Math.min(image.naturalHeight - y, Math.round(box.h * scale));
  if (width <= 0 || height <= 0) throw new Error('The selected region is outside the captured page.');
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Could not prepare the selected screenshot.');
  context.drawImage(image, x, y, width, height, 0, 0, width, height);
  return canvas.toDataURL('image/png');
}
