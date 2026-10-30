// JSON inside a text, shown as JSON: an error or a log line often carries one, either as the body after a message
// (`400 Bad Request: {...}`) or as a string holding escaped JSON (`"message": "... {\"error\": ...}"`). The JSON is found by
// its brackets, read, and printed indented; a JSON string inside it is read again. Free of stores so plain node can test it.

/** A text longer than this is shown as it is (a huge output is not searched for brackets). */
const LIMIT = 200_000;
/** How many levels of JSON strings inside JSON are read. */
const DEPTH = 6;

type Part = { raw: string } | { value: unknown };

/** The index just past the bracket that opens at `start` (outside strings), or -1 when it never closes. */
function closeOf(text: string, start: number): number {
  const stack: string[] = [];
  let inString = false;
  for (let index = start; index < text.length && index - start < LIMIT; index++) {
    const char = text[index];
    if (inString) {
      if (char === '\\') index++;
      else if (char === '"') inString = false;
    } else if (char === '"') {
      inString = true;
    } else if (char === '{' || char === '[') {
      stack.push(char === '{' ? '}' : ']');
    } else if (char === '}' || char === ']') {
      if (stack.pop() !== char) return -1;
      if (stack.length === 0) return index + 1;
    }
  }
  return -1;
}

/** The value of a JSON object or array with something in it: an empty `{}` or `[]` is left as text. */
function jsonValue(json: string): unknown {
  try {
    const value: unknown = JSON.parse(json);
    return typeof value === 'object' && value !== null && Object.keys(value).length > 0 ? value : undefined;
  } catch {
    return undefined;
  }
}

/** The text split into plain text and the JSON values found in it. */
function partsOf(text: string): Part[] {
  const parts: Part[] = [];
  let raw = '';
  let found = 0;
  for (let index = 0; index < text.length; ) {
    const char = text[index];
    if ((char === '{' || char === '[') && found < 50) {
      const end = closeOf(text, index);
      const value = end > 0 ? jsonValue(text.slice(index, end)) : undefined;
      if (value !== undefined) {
        if (raw) parts.push({ raw });
        raw = '';
        parts.push({ value });
        found++;
        index = end;
        continue;
      }
    }
    raw += char;
    index++;
  }
  if (raw) parts.push({ raw });
  return parts;
}

/** A JSON value printed with indentation, its JSON strings read as JSON too. */
function show(value: unknown, pad: string, depth: number): string {
  if (depth > DEPTH) return JSON.stringify(value) ?? '';
  if (typeof value === 'string') return showString(value, pad, depth);
  if (value === null || typeof value !== 'object') return JSON.stringify(value) ?? 'null';
  const inner = `${pad}  `;
  if (Array.isArray(value)) {
    if (value.length === 0) return '[]';
    return `[\n${value.map((item) => inner + show(item, inner, depth + 1)).join(',\n')}\n${pad}]`;
  }
  const keys = Object.keys(value);
  if (keys.length === 0) return '{}';
  const record = value as Record<string, unknown>;
  return `{\n${keys.map((key) => `${inner}${JSON.stringify(key)}: ${show(record[key], inner, depth + 1)}`).join(',\n')}\n${pad}}`;
}

/** A string: as a string when it holds no JSON, its JSON read when it is only JSON, and both when it mixes them. */
function showString(text: string, pad: string, depth: number): string {
  const parts = partsOf(text);
  if (!parts.some((part) => 'value' in part)) return JSON.stringify(text);
  if (parts.length === 1 && 'value' in parts[0]) return show(parts[0].value, pad, depth + 1);
  return parts.map((part) => ('raw' in part ? part.raw : show(part.value, pad, depth + 1))).join('');
}

/** The text with the JSON in it printed as indented JSON; the text as it is when it holds no JSON. */
export function jsonInText(text: string): string {
  if (text.length > LIMIT || !/[{[]/.test(text)) return text;
  const parts = partsOf(text);
  if (!parts.some((part) => 'value' in part)) return text;
  return parts.map((part) => ('raw' in part ? part.raw : show(part.value, '', 1))).join('');
}
