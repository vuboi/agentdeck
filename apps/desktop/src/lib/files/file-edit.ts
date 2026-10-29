// Pure helpers for editing a file in place: the editor works on text with "\n" line ends and no BOM, and the
// original style (BOM, CRLF) is put back when the file is written.

export type FileFormat = { crlf: boolean; bom: boolean };

/** The text as the editor holds it, and the BOM / line-end style to restore on save. */
export function splitFileText(raw: string): { text: string; format: FileFormat } {
  const bom = raw.charCodeAt(0) === 0xfeff;
  const body = bom ? raw.slice(1) : raw;
  const crlf = body.includes('\r\n');
  const text = body.includes('\r') ? body.replace(/\r\n?/g, '\n') : body;
  return { text, format: { crlf, bom } };
}

/** The bytes (as a string) to write for editor text, in the file's original style. */
export function joinFileText(text: string, format: FileFormat): string {
  const body = format.crlf ? text.replace(/\n/g, '\r\n') : text;
  return format.bom ? '\uFEFF' + body : body;
}

/** Files above this many bytes are not edited in place (the read-only viewer shows them). */
export const MAX_EDIT_BYTES = 1024 * 1024;
