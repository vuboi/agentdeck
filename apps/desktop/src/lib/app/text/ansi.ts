// Terminal output as text with colours: SGR codes become styled segments, every other escape sequence is dropped.

export type Segment = { text: string; color?: string; bold?: boolean; dim?: boolean };

// Mid-tone colours that read on both the light and the dark theme.
const BASIC = ['#6b7280', '#e5484d', '#30a46c', '#d9a400', '#3b82f6', '#c264d6', '#14a3b8', '#9ca3af'];
const BRIGHT = ['#8b919c', '#f2555a', '#3dd68c', '#f5c518', '#5b9cf8', '#d77ee8', '#2cc3d9', '#d1d5db'];

function indexed(n: number): string | undefined {
  if (n < 8) return BASIC[n];
  if (n < 16) return BRIGHT[n - 8];
  if (n < 232) {
    const level = (v: number) => (v === 0 ? 0 : 55 + v * 40);
    const c = n - 16;
    return `rgb(${level(Math.floor(c / 36))},${level(Math.floor(c / 6) % 6)},${level(c % 6)})`;
  }
  const grey = 8 + (n - 232) * 10;
  return `rgb(${grey},${grey},${grey})`;
}

// eslint-disable-next-line no-control-regex
const ESCAPES = /\x1b\[([0-9;:]*)([ -/]*[@-~])|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]/g;

/** Splits `text` into coloured segments. A carriage return rewrites its line, as in a terminal. */
export function parseAnsi(raw: string): Segment[] {
  const text = raw.replace(/\r\n/g, '\n');
  const out: Segment[] = [];
  let style: Omit<Segment, 'text'> = {};
  let last = 0;
  const push = (chunk: string) => {
    if (chunk) out.push({ text: chunk, ...style });
  };
  for (const match of text.matchAll(ESCAPES)) {
    push(text.slice(last, match.index));
    last = match.index + match[0].length;
    if (match[2] !== 'm') continue;
    const codes = (match[1] || '0').split(/[;:]/).map((part) => Number(part) || 0);
    for (let i = 0; i < codes.length; i++) {
      const code = codes[i];
      if (code === 0) style = {};
      else if (code === 1) style = { ...style, bold: true };
      else if (code === 2) style = { ...style, dim: true };
      else if (code === 22) style = { ...style, bold: false, dim: false };
      else if (code >= 30 && code <= 37) style = { ...style, color: BASIC[code - 30] };
      else if (code >= 90 && code <= 97) style = { ...style, color: BRIGHT[code - 90] };
      else if (code === 39) style = { ...style, color: undefined };
      else if (code === 38 && codes[i + 1] === 5) {
        style = { ...style, color: indexed(codes[i + 2] ?? 0) };
        i += 2;
      } else if (code === 38 && codes[i + 1] === 2) {
        style = { ...style, color: `rgb(${codes[i + 2] ?? 0},${codes[i + 3] ?? 0},${codes[i + 4] ?? 0})` };
        i += 4;
      }
    }
  }
  push(text.slice(last));
  // Progress bars rewrite their line with \r: only what follows the last one stays.
  for (const segment of out) {
    segment.text = segment.text
      .split('\n')
      .map((line) => line.slice(line.lastIndexOf('\r') + 1))
      .join('\n');
  }
  return out.filter((segment) => segment.text);
}

/** The plain text of `text`, with the colour codes taken out (for the copy button). */
export const stripAnsi = (text: string) => parseAnsi(text).map((segment) => segment.text).join('');
