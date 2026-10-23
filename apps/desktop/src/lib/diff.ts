// A unified diff (the hunks `change_diff` gives) as rows a viewer can draw: each line with its numbers in the old and
// the new file, and whether it was added, removed or is unchanged context.

export type DiffRow = { kind: 'hunk' | 'add' | 'del' | 'ctx'; old?: number; now?: number; text: string };

export function parseDiff(diff: string): DiffRow[] {
  const rows: DiffRow[] = [];
  let old = 0;
  let now = 0;
  for (const line of diff.split('\n')) {
    const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@\s?(.*)$/);
    if (hunk) {
      old = Number(hunk[1]);
      now = Number(hunk[2]);
      rows.push({ kind: 'hunk', text: `@@ -${hunk[1]} +${hunk[2]} @@ ${hunk[3]}`.trimEnd() });
    } else if (rows.length === 0 || line.startsWith('\\')) {
      continue;
    } else if (line.startsWith('+')) rows.push({ kind: 'add', now: now++, text: line.slice(1) });
    else if (line.startsWith('-')) rows.push({ kind: 'del', old: old++, text: line.slice(1) });
    else if (line.startsWith(' ') || line === '') rows.push({ kind: 'ctx', old: old++, now: now++, text: line.slice(1) });
  }
  // The split leaves one empty line after the last newline: it is not a line of the file.
  if (rows.at(-1)?.kind === 'ctx' && rows.at(-1)?.text === '' && diff.endsWith('\n')) rows.pop();
  return rows;
}

export const countsOf = (rows: DiffRow[]) => ({ added: rows.filter((row) => row.kind === 'add').length, removed: rows.filter((row) => row.kind === 'del').length });

/** The lines of the new file that were added or changed, for marking them in the plain view of the file. */
export const addedLines = (rows: DiffRow[]) => new Set(rows.filter((row) => row.kind === 'add' && row.now !== undefined).map((row) => row.now!));

/** Where a file is in a project folder, as git names it (forward slashes, no leading slash). */
export function relativeTo(root: string, path: string): string {
  const clean = (text: string) => text.replace(/\\/g, '/').replace(/\/+$/, '');
  const [r, p] = [clean(root), clean(path)];
  return p.toLowerCase().startsWith(r.toLowerCase() + '/') ? p.slice(r.length + 1) : p;
}
