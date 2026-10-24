<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { EditorHandle } from '../editor-cm';
  import { theme } from '../theme.svelte';

  // Editable source view (CodeMirror 6, loaded on first use). `doc` is the text it starts with: after that the
  // editor owns the text, and the parent reads it with getText() / changes it with setText(). The find query
  // marks every match and `current` is the one shown stronger.
  let {
    doc,
    lang,
    added,
    removed,
    modified,
    wrap = true,
    query = '',
    matchCase = false,
    wholeWord = false,
    regex = false,
    current = 0,
    total = $bindable(0),
    invalid = $bindable(false),
    onchange,
    onsave,
    onready,
  }: {
    doc: string;
    lang?: string;
    /** Lines (1-based) a change added: marked in green, until the text is edited. */
    added?: ReadonlySet<number>;
    /** Lines (1-based) where lines were taken out: marked in red. */
    removed?: ReadonlySet<number>;
    /** Lines (1-based) that replace others: marked in blue. */
    modified?: ReadonlySet<number>;
    wrap?: boolean;
    query?: string;
    matchCase?: boolean;
    wholeWord?: boolean;
    regex?: boolean;
    current?: number;
    total?: number;
    invalid?: boolean;
    onchange?: () => void;
    onsave?: () => void;
    onready?: () => void;
  } = $props();

  let host: HTMLDivElement | undefined = $state();
  let handle = $state.raw<EditorHandle | null>(null);

  // The text at the moment the editor went away, so a parent that is torn down after it can still read it.
  let lastText: string | null = null;
  export const getText = () => handle?.text() ?? lastText;
  /** Number of lines now. */
  export const lineCount = () => handle?.view.state.doc.lines ?? 0;
  export const equals = (text: string) => handle?.equals(text) ?? true;
  export const setText = (text: string, undoable = false) => handle?.setText(text, undoable);
  export const focus = () => handle?.focus();
  export const goToLine = (line: number) => handle?.goTo(line);

  onMount(() => {
    let dead = false;
    let made: EditorHandle | undefined;
    void import('../editor-cm').then(({ createEditor }) => {
      if (dead || !host) return;
      made = createEditor(host, {
        doc,
        lang,
        dark: theme.dark,
        wrap,
        onChange: () => onchange?.(),
        onSave: () => onsave?.(),
        onFind: (count, bad) => {
          total = count;
          invalid = bad;
        },
      });
      handle = made;
      onready?.();
    });
    return () => {
      dead = true;
      if (made) lastText = made.text();
      made?.destroy();
      handle = null;
    };
  });

  $effect(() => {
    const on = wrap;
    untrack(() => handle)?.setWrap(on);
  });
  $effect(() => {
    const dark = theme.dark;
    untrack(() => handle)?.setDark(dark);
  });
  $effect(() => {
    const next = lang;
    untrack(() => handle)?.setLang(next);
  });
  $effect(() => {
    const marks = added;
    const cut = removed;
    const changed = modified;
    handle?.setAdded(marks, cut, changed);
  });
  $effect(() => {
    const cfg = { query, matchCase, wholeWord, regex };
    const at = current;
    handle?.find(cfg, at);
    if (!cfg.query) {
      total = 0;
      invalid = false;
    }
  });
</script>

<div bind:this={host} class="h-full min-h-0 [&_.cm-editor]:h-full"></div>
