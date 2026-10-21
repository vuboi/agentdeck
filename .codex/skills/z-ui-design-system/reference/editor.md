# Editor — `z-editor`

Selector: `z-editor`. A rich-text editor built on TipTap/ProseMirror with a fixed toolbar, a selection bubble
menu, slash commands (`/`), emoji picker, link editing popover, and image upload — wired up as a full Angular
`ControlValueAccessor` (works with `ngModel`/reactive forms) with the same label/required/validation UX as
`z-input`. Never drop a raw `contenteditable` div or a third-party editor unwrapped into a form.

```html
<z-editor zLabel="Description" [(ngModel)]="content" [zRequired]="true" />
```

```html
<!-- Reactive forms -->
<z-editor formControlName="body" zPlaceholder="Write something…" />
```

## Inputs

| Input          | Type                                     | Default                    | Notes                                                                                           |
| -------------- | ---------------------------------------- | -------------------------- | ----------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                             | `''`                       | Extra classes merged with variant classes                                                       |
| `zSize`        | `'sm' \| 'default' \| 'lg'`              | `'default'`                | Min-height + font size of the editing surface (see table below)                                 |
| `zLabel`       | `string`                                 | `''`                       | Field label above the editor                                                                    |
| `zLabelClass`  | `string`                                 | `''`                       | Extra classes on the label element                                                              |
| `zPlaceholder` | `string`                                 | `''`                       | Empty-state placeholder (falls back to i18n default when unset)                                 |
| `zRequired`    | `boolean` (via `zTransform`)             | `false`                    | Marks label with `*` and fails validation when the editor is empty                              |
| `zDisabled`    | `boolean` (via `zTransform`)             | `false`                    | Disables editing, dims the surface                                                              |
| `zReadonly`    | `boolean` (via `zTransform`)             | `false`                    | Read-only (not editable, but not visually disabled)                                             |
| `zContentType` | `'html' \| 'json' \| 'markdown'`         | `'html'`                   | Serialization format used for the `ControlValueAccessor` value and `zOnChange`                  |
| `zToolbar`     | `readonly ZEditorToolbarItem[] \| false` | `Z_EDITOR_DEFAULT_TOOLBAR` | Customize/reorder the fixed toolbar, or pass `false` to hide it                                 |
| `zTipTap`      | `ZEditorTipTapOptions`                   | `{}`                       | Escape hatch into TipTap: `starterKit`, `placeholder`, `image`, `link`, `mention`, `extensions` |
| `zValidators`  | `ZEditorValidator[]`                     | `[]`                       | Custom validators: `{ error, message, pattern?, minLength?, maxLength? }`                       |

## Outputs

| Output      | Payload             | Fires when                                                                              |
| ----------- | ------------------- | --------------------------------------------------------------------------------------- |
| `zOnChange` | `string \| Content` | Editor content changes (serialized per `zContentType`)                                  |
| `zOnFocus`  | `FocusEvent`        | Editor gains focus                                                                      |
| `zOnBlur`   | `FocusEvent`        | Editor loses focus                                                                      |
| `zControl`  | `ZEditorControl`    | Emitted once the TipTap editor is created — exposes an imperative API (see below)       |
| `zEvent`    | `ZEvent`            | Generic wrapped focus/blur event, for uniform event pipelines across z-ui form controls |

## `ZEditorControl` — imperative API via `(zControl)`

`zControl` emits a `ZEditorControl` object once; capture it to drive the editor programmatically:
`focus()`, `blur()`, `clear()`, `setContent(content)` / `setValue(content)`, `getContent()`, `getHTML()`,
`getJSON()`, `getMarkdown()`, `getText()`, `getLength()`, `getEditor()` (raw TipTap `Editor` instance),
`valid()` / `validate()` (deprecated alias), `reset()`, `setErrorMessage(message | null)`, and read-only
signals `hasError`, `value`, `errorMessage`.

```html
<z-editor (zControl)="editorControl = $event" [(ngModel)]="content" />
<button z-button (click)="editorControl.clear()">Clear</button>
```

## `zSize` — editing-surface min-height

From `z-editor.variants.ts` (`[&_.z-editor-prosemirror]` targets the contenteditable area):

| `zSize`   | Min-height         | Font        |
| --------- | ------------------ | ----------- |
| `sm`      | `min-h-24` (96px)  | `text-sm`   |
| `default` | `min-h-40` (160px) | `text-sm`   |
| `lg`      | `min-h-60` (240px) | `text-base` |

There is no `zType`/visual-style variant — status styling (`default`/`error`/`disabled`/`readonly`) is driven
internally by `zRequired`/validators/`zDisabled`/`zReadonly`, not by a prop you set directly.

## `zToolbar` — customize the fixed toolbar

`Z_EDITOR_DEFAULT_TOOLBAR` (from `z-editor.define.ts`) includes, in order: `undo`, `redo`, `bold`, `italic`,
`underline`, `strike`, `code`, `blockStyle`, `bulletList`, `orderedList`, `blockquote`, `codeBlock`, `link`,
`image`, `emoji`, `textColor`, `backgroundColor`, `align`, `horizontalRule`, `clearFormatting`. Pass a filtered
subset (each item is `{ id: ZEditorToolbarCommand, icon: ZIcon, label: string }`) to trim it, or `[zToolbar]="false"`
to hide the toolbar entirely (the selection bubble menu still appears on text selection regardless of `zToolbar`).

## `zTipTap` — escape hatch

`ZEditorTipTapOptions`: `starterKit` (partial `StarterKitOptions`), `placeholder` (string or
`{ mode: 'firstLine' | 'everyLine', ...PlaceholderOptions }`), `image` (`false` to disable image support, or
options object), `link` (partial `LinkOptions`), `mention` (`false` to disable, or `MentionOptions` for
`@mention` suggestions), `extensions` (additional raw TipTap `Extension[]`).

## Examples

```html
<!-- Basic, form-bound -->
<z-editor zLabel="Body" [(ngModel)]="post.body" [zRequired]="true" />

<!-- Minimal toolbar, small size, no image/mention -->
<z-editor
  zSize="sm"
  [zToolbar]="[{ id: 'bold', icon: 'lucideBold', label: 'Bold' }, { id: 'italic', icon: 'lucideItalic', label: 'Italic' }]"
  [zTipTap]="{ image: false, mention: false }"
  [(ngModel)]="comment"
/>

<!-- Markdown content type -->
<z-editor zContentType="markdown" [(ngModel)]="markdownSource" />

<!-- Custom min/max length validators -->
<z-editor
  [zValidators]="[{ error: 'maxlength', message: 'Max 500 characters', maxLength: 500 }]"
  [(ngModel)]="summary"
/>
```

## Anti-patterns

```html
<!-- WRONG: raw contenteditable, no toolbar, no form integration, no validation -->
<div contenteditable="true" class="rounded border p-2"></div>

<!-- WRONG: invented prop, size values must match the real union -->
<z-editor zSize="medium" />

<!-- WRONG: trying to read content via a template ref method that doesn't exist -->
<z-editor #ed></z-editor>
<button (click)="ed.getValue()">Save</button>
```

```html
<!-- RIGHT: capture the imperative control via (zControl), then call its methods -->
<z-editor (zControl)="editorControl = $event" [(ngModel)]="content"></z-editor>
<button z-button (click)="editorControl.getHTML()">Save</button>
```
