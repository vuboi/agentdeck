// CodeMirror 6 editor for the file viewer. Loaded on demand (dynamic import from CodeEditor.svelte), so the
// editor never weighs on startup. Syntax colours are the same shiki (GitHub) tokens the read-only view uses, laid
// over the text as mark decorations: the colours stay identical in both views and no language packages are shipped.
import { Compartment, EditorState, RangeSet, RangeSetBuilder, StateEffect, StateField, Transaction, type Extension } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, drawSelection, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumberMarkers, lineNumbers, type DecorationSet } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { highlightTokens, type Token } from './highlight';

export type FindConfig = { query: string; matchCase: boolean; wholeWord: boolean; regex: boolean };
export type EditorOptions = {
  doc: string;
  lang?: string;
  dark: boolean;
  wrap: boolean;
  /** The document changed (typing, undo, or text put in by the app). */
  onChange: () => void;
  onSave: () => void;
  /** Number of find matches and whether the query is a regular expression that does not parse. */
  onFind: (total: number, invalid: boolean) => void;
};

const MAX_MATCHES = 20_000;

// ---------- find ----------
type Range = { from: number; to: number };
type FindState = { cfg: FindConfig; ranges: Range[]; invalid: boolean; current: number; deco: DecorationSet };
const noFind: FindConfig = { query: '', matchCase: false, wholeWord: false, regex: false };
const setFind = StateEffect.define<{ cfg: FindConfig; current: number }>();
const hit = Decoration.mark({ class: 'cm-find' });
const hitCurrent = Decoration.mark({ class: 'cm-find cm-find-current' });

/** The matches of a query, line by line (a match never spans lines), at most MAX_MATCHES. */
export function findMatches(doc: EditorState['doc'], cfg: FindConfig): { ranges: Range[]; invalid: boolean } {
  if (!cfg.query) return { ranges: [], invalid: false };
  let pattern: RegExp;
  try {
    const body = cfg.regex ? cfg.query : cfg.query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    pattern = new RegExp(cfg.wholeWord ? `\\b(?:${body})\\b` : body, cfg.matchCase ? 'g' : 'gi');
  } catch {
    return { ranges: [], invalid: true };
  }
  const ranges: Range[] = [];
  for (let n = 1; n <= doc.lines && ranges.length < MAX_MATCHES; n++) {
    const line = doc.line(n);
    pattern.lastIndex = 0;
    for (let m = pattern.exec(line.text); m && ranges.length < MAX_MATCHES; m = pattern.exec(line.text)) {
      if (m[0] === '') pattern.lastIndex += 1;
      else ranges.push({ from: line.from + m.index, to: line.from + m.index + m[0].length });
    }
  }
  return { ranges, invalid: false };
}

const findField = StateField.define<FindState>({
  create: () => ({ cfg: noFind, ranges: [], invalid: false, current: 0, deco: Decoration.none }),
  update(value, tr) {
    let { cfg, current } = value;
    let recompute = tr.docChanged && Boolean(cfg.query);
    for (const effect of tr.effects) {
      if (effect.is(setFind)) {
        cfg = effect.value.cfg;
        current = effect.value.current;
        recompute = true;
      }
    }
    if (!recompute) return value;
    const { ranges, invalid } = findMatches(tr.state.doc, cfg);
    if (current >= ranges.length) current = 0;
    const deco = Decoration.set(ranges.map((range, index) => (index === current ? hitCurrent : hit).range(range.from, range.to)));
    return { cfg, ranges, invalid, current, deco };
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.deco),
});

// ---------- colours ----------
const setTokens = StateEffect.define<DecorationSet>();
const markCache = new Map<string, Decoration>();
const colorMark = (color: string) => {
  let mark = markCache.get(color);
  if (!mark) markCache.set(color, (mark = Decoration.mark({ attributes: { style: `color:${color}` } })));
  return mark;
};
const tokenField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    value = value.map(tr.changes);
    for (const effect of tr.effects) if (effect.is(setTokens)) value = effect.value;
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

function tokenDecorations(doc: EditorState['doc'], rows: Token[][]): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (let i = 0; i < rows.length && i < doc.lines; i++) {
    const line = doc.line(i + 1);
    let offset = line.from;
    for (const token of rows[i]) {
      const end = offset + token.content.length;
      if (token.color && end > offset && end <= line.to) builder.add(offset, end, colorMark(token.color));
      offset = end;
    }
  }
  return builder.finish();
}

// ---------- added lines ----------
class NumberMark extends GutterMarker {
  constructor(readonly elementClass: string) {
    super();
  }
  eq(other: NumberMark) {
    return other.elementClass === this.elementClass;
  }
}
const addedNumber = new NumberMark('cm-added-number');
const removedNumber = new NumberMark('cm-removed-number');
const modifiedNumber = new NumberMark('cm-modified-number');
const addedLine = Decoration.line({ class: 'cm-added-line' });
const removedLine = Decoration.line({ class: 'cm-removed-line' });
const modifiedLine = Decoration.line({ class: 'cm-modified-line' });
function addedExtension(doc: EditorState['doc'], added?: ReadonlySet<number>, removed?: ReadonlySet<number>, modified?: ReadonlySet<number>): Extension {
  const kinds = new Map<number, 'add' | 'mod' | 'del'>();
  for (const n of removed ?? []) kinds.set(n, 'del');
  for (const n of added ?? []) kinds.set(n, 'add');
  for (const n of modified ?? []) kinds.set(n, 'mod');
  const lines = [...kinds.keys()].filter((n) => n >= 1 && n <= doc.lines).sort((a, b) => a - b);
  if (lines.length === 0) return [];
  const pick = <T,>(n: number, add: T, mod: T, del: T) => (kinds.get(n) === 'add' ? add : kinds.get(n) === 'mod' ? mod : del);
  return [
    EditorView.decorations.of(Decoration.set(lines.map((n) => pick(n, addedLine, modifiedLine, removedLine).range(doc.line(n).from)))),
    lineNumberMarkers.of(RangeSet.of(lines.map((n) => pick(n, addedNumber, modifiedNumber, removedNumber).range(doc.line(n).from)))),
  ];
}

// ---------- look ----------
const theme = (dark: boolean) =>
  EditorView.theme(
    {
      '&': { height: '100%', backgroundColor: 'var(--background)', color: 'color-mix(in srgb, var(--foreground) 90%, transparent)', fontSize: '12px' },
      '&.cm-focused': { outline: 'none' },
      '.cm-scroller': { fontFamily: 'var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace)', lineHeight: '20px', overflow: 'auto' },
      '.cm-content': { caretColor: 'var(--foreground)', padding: '0' },
      '.cm-line': { padding: '0 24px 0 4px' },
      '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--foreground)' },
      '.cm-gutters': { backgroundColor: 'var(--background)', color: 'color-mix(in srgb, var(--muted-foreground) 60%, transparent)', border: 'none', userSelect: 'none' },
      '.cm-lineNumbers .cm-gutterElement': { minWidth: '52px', padding: '0 12px 0 0' },
      '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--foreground) 5%, transparent)' },
      '.cm-activeLineGutter': { backgroundColor: 'color-mix(in srgb, var(--foreground) 5%, transparent)', color: 'var(--foreground)' },
      '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: 'color-mix(in srgb, var(--primary) 30%, transparent) !important' },
      '.cm-find': { backgroundColor: 'color-mix(in srgb, var(--primary) 25%, transparent)', borderRadius: '2px' },
      '.cm-find-current': { backgroundColor: 'color-mix(in srgb, var(--primary) 60%, transparent)', outline: '1px solid var(--primary)' },
      '.cm-removed-line': { backgroundColor: 'rgb(239 68 68 / 0.14)' },
      '.cm-added-line': { backgroundColor: 'rgb(16 185 129 / 0.14)' },
      '.cm-modified-line': { backgroundColor: 'rgb(59 130 246 / 0.14)' },
      '.cm-added-number': { position: 'relative', fontWeight: '700', color: '#10b981', boxShadow: 'inset 2px 0 0 #10b981' },
      '.cm-modified-number': { fontWeight: '700', color: '#3b82f6', boxShadow: 'inset 2px 0 0 #3b82f6' },
      '.cm-removed-number': { position: 'relative', fontWeight: '700', color: '#ef4444', boxShadow: 'inset 2px 0 0 #ef4444' },
      '.cm-removed-number::before': { content: '"−"', position: 'absolute', left: '6px', fontWeight: '700' },
      '.cm-added-number::before': { content: '"+"', position: 'absolute', left: '6px', fontWeight: '700' },
    },
    { dark },
  );

export type EditorHandle = {
  view: EditorView;
  text(): string;
  /** True when the document is exactly `text` (cheap length check first). */
  equals(text: string): boolean;
  /** Replaces the document, touching only what differs, so the cursor and scroll stay put. `undoable`: Ctrl+Z reverts it. */
  setText(text: string, undoable?: boolean): void;
  setWrap(on: boolean): void;
  setDark(on: boolean): void;
  setLang(lang?: string): void;
  /** Lines (1-based) to mark as added; none clears the marks. */
  setAdded(added?: ReadonlySet<number>, removed?: ReadonlySet<number>, modified?: ReadonlySet<number>): void;
  /** Marks every match of the query and shows match `current` as the current one, scrolled into view. */
  find(cfg: FindConfig, current: number): void;
  focus(): void;
  /** Scrolls a line (1-based) to the middle of the view. */
  goTo(line: number): void;
  destroy(): void;
};

export function createEditor(parent: HTMLElement, options: EditorOptions): EditorHandle {
  let { lang, dark } = options;
  const wrapping = new Compartment();
  const look = new Compartment();
  const marks = new Compartment();
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        history(),
        drawSelection(),
        lineNumbers(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        keymap.of([{ key: 'Mod-s', preventDefault: true, run: () => (options.onSave(), true) }, ...defaultKeymap, ...historyKeymap, indentWithTab]),
        findField,
        tokenField,
        wrapping.of(options.wrap ? EditorView.lineWrapping : []),
        look.of(theme(dark)),
        marks.of([]),
        EditorView.contentAttributes.of({ spellcheck: 'false', autocorrect: 'off', autocapitalize: 'off' }),
        EditorView.updateListener.of((update) => {
          const before = update.startState.field(findField);
          const after = update.state.field(findField);
          if (before !== after && (before.ranges.length !== after.ranges.length || before.invalid !== after.invalid)) options.onFind(after.ranges.length, after.invalid);
          if (update.docChanged) {
            options.onChange();
            clearTimeout(timer);
            timer = setTimeout(() => void colorize(), 250);
          }
        }),
      ],
    }),
  });

  /** Colours the whole text in chunks (a first small one so the top is coloured at once); an edit in between starts over. */
  async function colorize() {
    const gen = ++generation;
    const doc = view.state.doc;
    if (!lang) return void view.dispatch({ effects: setTokens.of(Decoration.none) });
    const lines = doc.toString().split('\n');
    const rows: Token[][] = [];
    for (let i = 0; i < lines.length; ) {
      const size = i === 0 ? 150 : 1500;
      const result = await highlightTokens(lines.slice(i, i + size).join('\n'), lang, dark);
      if (gen !== generation || view.state.doc !== doc || !result) return; // no result: unknown language, the text stays plain
      rows.push(...result);
      i += size;
      view.dispatch({ effects: setTokens.of(tokenDecorations(doc, rows)) });
      if (i < lines.length) await new Promise((resolve) => setTimeout(resolve, 0));
      if (gen !== generation) return;
    }
  }
  void colorize();

  return {
    view,
    text: () => view.state.doc.toString(),
    equals: (text) => view.state.doc.length === text.length && view.state.doc.toString() === text,
    setText(text, undoable = false) {
      const old = view.state.doc.toString();
      if (old === text) return;
      let start = 0;
      const max = Math.min(old.length, text.length);
      while (start < max && old.charCodeAt(start) === text.charCodeAt(start)) start++;
      let end = 0;
      while (end < max - start && old.charCodeAt(old.length - 1 - end) === text.charCodeAt(text.length - 1 - end)) end++;
      view.dispatch({
        changes: { from: start, to: old.length - end, insert: text.slice(start, text.length - end) },
        annotations: undoable ? [] : [Transaction.addToHistory.of(false)],
      });
    },
    setWrap: (on) => view.dispatch({ effects: wrapping.reconfigure(on ? EditorView.lineWrapping : []) }),
    setDark(on) {
      dark = on;
      view.dispatch({ effects: look.reconfigure(theme(on)) });
      void colorize();
    },
    setLang(next) {
      lang = next;
      void colorize();
    },
    setAdded: (added, removed, modified) => view.dispatch({ effects: marks.reconfigure(addedExtension(view.state.doc, added, removed, modified)) }),
    find(cfg, current) {
      view.dispatch({ effects: setFind.of({ cfg, current }) });
      const range = view.state.field(findField).ranges[current];
      if (range && cfg.query) view.dispatch({ effects: EditorView.scrollIntoView(range.from, { y: 'center' }) });
    },
    focus: () => view.focus(),
    goTo: (line) => view.dispatch({ effects: EditorView.scrollIntoView(view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines)).from, { y: 'center' }) }),
    destroy() {
      clearTimeout(timer);
      generation++;
      view.destroy();
    },
  };
}
