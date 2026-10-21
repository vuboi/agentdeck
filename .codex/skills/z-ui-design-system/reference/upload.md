# Upload — `z-upload`

Selector: `z-upload`, `exportAs="zUpload"`. A dropzone + file-list control implementing `ControlValueAccessor`
(works with `formControlName` / `ngModel`, value is `ZUploadFile[]`). Never build a file dropzone with a bare
`<input type="file">` and hand-rolled drag handlers.

```html
<z-upload
  formControlName="attachments"
  zLabel="Attachments"
  [zMultipleFile]="true"
  [zMaxFiles]="5"
  zAcceptFile="image/*,.pdf"
/>
```

## `ZUploadFile` (the control's value type, and `zOnUpload`/`zOnRemove` payload item)

| Field      | Type                                               | Notes                                                                                 |
| ---------- | -------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `id`       | `string`                                           | Auto-generated                                                                        |
| `name`     | `string`                                           |                                                                                       |
| `size`     | `number`                                           | Bytes                                                                                 |
| `type`     | `string`                                           | MIME type                                                                             |
| `file`     | `File`                                             | Present for newly-picked files                                                        |
| `url`      | `string`                                           | Present for pre-existing/remote files (used by `downloadFile` when there's no `file`) |
| `progress` | `number`                                           | 0-100, update via `updateProgress()`                                                  |
| `status`   | `'pending' \| 'uploading' \| 'success' \| 'error'` |                                                                                       |

## Inputs

| Input           | Type                        | Default     | Notes                                                                                                |
| --------------- | --------------------------- | ----------- | ---------------------------------------------------------------------------------------------------- |
| `class`         | `ClassValue`                | `''`        | Extra classes on the dropzone                                                                        |
| `zLabel`        | `string`                    | `''`        | Field label                                                                                          |
| `zLabelClass`   | `string`                    | `''`        | Extra classes on the label                                                                           |
| `zNote`         | `string`                    | `''`        | Helper text under the dropzone (e.g. accepted types/size hint)                                       |
| `zSize`         | `'sm' \| 'default' \| 'lg'` | `'default'` | Dropzone min-height/padding                                                                          |
| `zAcceptFile`   | `string`                    | `'*'`       | Comma-separated accept list: extensions (`.pdf`), MIME types (`image/png`), or wildcards (`image/*`) |
| `zMultipleFile` | `boolean`                   | `false`     | Allow multiple files (single-file mode replaces the current file on new selection)                   |
| `zMaxSize`      | `number`                    | `10`        | Max size per file, in **MB**                                                                         |
| `zMaxFiles`     | `number`                    | `5`         | Max file count (only enforced when `zMultipleFile`)                                                  |
| `zRequired`     | `boolean`                   | `false`     | Adds a "required" validation error when empty                                                        |
| `zDisabled`     | `boolean`                   | `false`     | Disabled state                                                                                       |
| `zReadonly`     | `boolean`                   | `false`     | Read-only (blocks new uploads/removal, distinct visual from disabled)                                |
| `zLoading`      | `boolean`                   | `false`     | Blocks interaction while an upload is in flight                                                      |
| `zShowProgress` | `boolean`                   | `true`      | Show per-file progress bar                                                                           |
| `zValidators`   | `ZUploadValidator[]`        | `[]`        | Custom validators: `{ error: string; message: string; validate?: (files) => boolean }`               |

## Outputs

| Output                                     | Payload                                          | Fires when                                                                      |
| ------------------------------------------ | ------------------------------------------------ | ------------------------------------------------------------------------------- |
| `zOnUpload`                                | `ZUploadFile[]`                                  | New valid files are accepted (picked or dropped)                                |
| `zOnRemove`                                | `ZUploadFile`                                    | A file is removed from the list                                                 |
| `zOnDownload`                              | `ZUploadFile`                                    | The download action on a file is triggered                                      |
| `zOnError`                                 | `ZUploadError` (`{ file: File; error: string }`) | A file fails a built-in check (size/type/max-files)                             |
| `zOnFileChange`                            | `File[]`                                         | The raw file list changes (after add/remove)                                    |
| `zOnDragover` / `zOnDragleave` / `zOnDrop` | `DragEvent`                                      | Native drag events on the dropzone                                              |
| `zControl`                                 | `ZUploadControl`                                 | Emitted once on init — a handle exposing imperative methods/signals (see below) |
| `zEvent`                                   | `ZEvent`                                         | Unified drag event wrapper                                                      |

## Public API (template ref via `#upload="zUpload"`, or the `zControl` output)

| Member                                                                         | Notes                                                                           |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| `files`                                                                        | `Signal<ZUploadFile[]>` — current files                                         |
| `getFiles(): File[]`                                                           | Raw `File` objects for files that were locally picked                           |
| `updateProgress(fileId, progress)`                                             | Sets progress and flips status to `'success'` at 100                            |
| `setFileStatus(fileId, status)`                                                | Manually set a file's status                                                    |
| `clearFiles()`                                                                 | Empty the list without full form reset                                          |
| `reset()`                                                                      | Clear files and reset touched/dirty/pristine state                              |
| `valid(): boolean`                                                             | Runs validation, returns `true` if valid                                        |
| `validate(): boolean`                                                          | **Deprecated** — same check, inverted return (`true` if invalid); use `valid()` |
| `markAsTouched()` / `markAsUntouched()` / `markAsDirty()` / `markAsPristine()` | Manual form-state control                                                       |
| `hasError`                                                                     | `Signal<boolean>`                                                               |
| `errorMessage`                                                                 | `Signal<string>`                                                                |
| `openFileDialog()`                                                             | Programmatically opens the native file picker                                   |

## `zSize` — dropzone padding

| `zSize`   | Classes              |
| --------- | -------------------- |
| `sm`      | `min-h-24 gap-1 p-3` |
| `default` | `min-h-32 gap-2 p-4` |
| `lg`      | `min-h-40 gap-3 p-6` |

## Examples

```html
<!-- Single required file, reactive form -->
<z-upload
  formControlName="resume"
  zLabel="Resume"
  [zRequired]="true"
  zAcceptFile=".pdf,.doc,.docx"
  zNote="PDF or Word, max 10MB"
/>

<!-- Multiple images, custom max size -->
<z-upload
  formControlName="photos"
  zLabel="Photos"
  [zMultipleFile]="true"
  [zMaxFiles]="8"
  [zMaxSize]="5"
  zAcceptFile="image/*"
/>

<!-- Manual progress reporting via template ref -->
<z-upload #upload="zUpload" (zOnUpload)="startUpload($event, upload)" />
```

```ts
startUpload(files: ZUploadFile[], upload: ZUploadComponent) {
  for (const f of files) {
    this.http.post(`/upload`, f.file).subscribe({
      next: () => upload.updateProgress(f.id, 100),
      error: () => upload.setFileStatus(f.id, 'error'),
    });
  }
}
```

## Anti-patterns

```html
<!-- WRONG: raw file input, no drag-drop, no size/type validation -->
<input type="file" multiple (change)="onFiles($event)" />

<!-- WRONG: invented input, zDragDrop is not a real prop (drag-drop is built in, always on) -->
<z-upload zDragDrop="true" />

<!-- WRONG: zMaxSize is megabytes, not bytes -->
<z-upload [zMaxSize]="10485760" />
```
