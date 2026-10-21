# Gallery — `z-gallery`

Selector: `z-gallery`, `exportAs="zGallery"`. A file/media gallery with grid and list modes, search,
multi-select with bulk actions, loading skeletons, and a built-in lightbox preview (image/PDF/video/audio/text/code
and Office preview content via `z-gallery-preview`, opened through an internal service). Never build a file grid by
hand — you lose preview, selection, and skeleton states.

```html
<z-gallery [zFiles]="files()" (zOnDownload)="onDownload($event)" (zOnRemove)="onRemove($event)" />
```

## Inputs

| Input                     | Type                                      | Default             | Notes                                                                                                                           |
| ------------------------- | ----------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `zFiles`                  | `ZGalleryFile[]`                          | `[]`                | The files to display (`id`, `name`, `size`, `type`, optional `url`, `downloadUrl`, `thumbnailUrl`, dates)                       |
| `zMode`                   | `model<ZGalleryMode>`: `'grid' \| 'list'` | `'grid'`            | Two-way bindable view mode                                                                                                      |
| `zSize`                   | `'sm' \| 'default' \| 'lg'`               | `'default'`         | Container/item density (via `zGalleryVariants`)                                                                                 |
| `zColumns`                | `number`                                  | `4`                 | Grid column count (`1`–`6` have explicit responsive breakpoint classes; other values fall back to a generated `lg:grid-cols-N`) |
| `zItemWidth`              | `string \| null`                          | `null`              | If set, switches the grid to `repeat(auto-fill, minmax(width, width))` instead of `zColumns`                                    |
| `zLoading`                | `boolean`                                 | `false`             | Shows skeleton placeholders instead of files                                                                                    |
| `zSkeletonRows`           | `number`                                  | `2`                 | Skeleton row count when `zSkeletonItems` isn't set (grid mode: `columns × rows` items)                                          |
| `zSkeletonItems`          | `number \| null`                          | `null`              | Explicit skeleton item count, overrides `zSkeletonRows` calculation                                                             |
| `zShowDownload`           | `boolean`                                 | `true`              | Show per-item and bulk download actions                                                                                         |
| `zShowPreview`            | `boolean`                                 | `true`              | Enable click-to-preview (lightbox)                                                                                              |
| `zShowRemove`             | `boolean`                                 | `false`             | Show per-item and bulk remove actions                                                                                           |
| `zPdfViewerConfig`        | `ZGalleryPdfViewerConfig`                 | `{}`                | Passed through to the PDF viewer used by the preview lightbox                                                                   |
| `zTrustedResourceOrigins` | `string[]`                                | `[]`                | Origins allowed for embedded preview resources                                                                                  |
| `zShowModeToggle`         | `boolean`                                 | `true`              | Show the grid/list toggle button group in the header                                                                            |
| `zShowSearch`             | `boolean`                                 | `false`             | Show the header search input                                                                                                    |
| `zServerSearch`           | `boolean`                                 | `false`             | If true, disables local filtering — you're expected to filter `zFiles` yourself in response to `zSearchChange`                  |
| `zSearchDebounce`         | `number`                                  | `300`               | Search input debounce (ms)                                                                                                      |
| `zSearchPlaceholder`      | `string`                                  | `'Search files...'` | Search input placeholder                                                                                                        |
| `zToggleSize`             | `'xs' \| 'sm' \| 'default' \| 'lg'`       | `'default'`         | Size of the header search input and mode-toggle buttons                                                                         |
| `zTitle`                  | `string`                                  | `''`                | Optional header title                                                                                                           |
| `zEmptyText`              | `string`                                  | `'No files'`        | Empty-state message                                                                                                             |
| `zEmptyIcon`              | `ZIcon`                                   | `'lucideImage'`     | Empty-state icon                                                                                                                |
| `zClass`                  | `ClassValue`                              | `''`                | Extra classes on the container                                                                                                  |
| `zSelectable`             | `boolean`                                 | `false`             | Enables checkbox multi-select mode with a bulk-action bar                                                                       |
| `zSelectedFiles`          | `model<ZGalleryFile[]>`                   | `[]`                | Two-way bindable selection — use `[(zSelectedFiles)]`                                                                           |

## Outputs

| Output             | Payload                                                                     | Fires when                                                                                                                   |
| ------------------ | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `zOnDownload`      | `ZGalleryFile`                                                              | A file's download action is triggered (the component also auto-triggers the actual browser download)                         |
| `zOnPreview`       | `ZGalleryFile`                                                              | A file is opened in the preview lightbox                                                                                     |
| `zOnRemove`        | `ZGalleryFile`                                                              | A file's remove action is triggered (item action or from within the preview)                                                 |
| `zSearchChange`    | `string`                                                                    | The search query changes (debounced)                                                                                         |
| `zControl`         | `ZGalleryControl`                                                           | Once, after view init — an imperative control object (see below)                                                             |
| `zSelectionChange` | `ZGallerySelectionChange` (`selectedFiles`, `addedFiles?`, `removedFiles?`) | Selection changes (single toggle, select-all, deselect-all)                                                                  |
| `zOnAction`        | `ZGalleryActionEvent` (`type`, `file?`, `files?`)                           | Any bulk/selection action: `'select' \| 'deselect' \| 'select-all' \| 'deselect-all' \| 'delete' \| 'download' \| 'preview'` |

## Imperative control (`zControl`)

`zControl` emits once with an object exposing `setMode`, `getMode`, `getFiles`, `openPreview`, `closePreview`,
`selectAll`, `deselectAll`, `toggleSelection`, `getSelectedFiles`, `isSelected` — capture it if you need to
drive the gallery from outside the template (e.g. a toolbar button elsewhere on the page) instead of relying
only on bindable inputs.

## `zSize` — density

| `zSize`   | Container class      |
| --------- | -------------------- |
| `sm`      | `z-gallery--sm`      |
| `default` | `z-gallery--default` |
| `lg`      | `z-gallery--lg`      |

Per-item grid/list padding and file-icon badge sizing also scale with `zSize` — see
[z-gallery.variants.ts](../../../../libs/core-ui/components/z-gallery/z-gallery.variants.ts) for the full
`zGalleryItemVariants`/`zGalleryFileIconVariants` compound-variant tables (e.g. list-mode padding is
`p-2`/`p-3`/`p-4` for `sm`/`default`/`lg`).

## Preview support

The gallery classifies files by MIME type or extension. Previewable categories are `image`, `pdf`, `video`, `audio`,
`word`, `excel`, `text`, and `code`; archives and unknown files remain download-only. The preview lightbox uses
`z-media-player` for video, `z-audio-player` for audio, `z-code` for recognized code files, and dedicated text/Office
preview components for the corresponding content.

For custom preview triggers, use the exported `zGalleryPreviewItem` / `z-gallery-preview-item` directive. It opens the
same preview service, supports keyboard activation, and accepts `zGalleryPreviewFiles` plus
`zGalleryPreviewOptions` for neighboring files and preview configuration.

## Examples

```html
<!-- Basic read-only gallery -->
<z-gallery [zFiles]="files()" />

<!-- Selectable gallery with bulk delete/download -->
<z-gallery
  [zFiles]="files()"
  zSelectable
  [(zSelectedFiles)]="selected"
  [zShowRemove]="true"
  (zOnAction)="onGalleryAction($event)"
/>

<!-- Search-enabled, list mode default, server-side search -->
<z-gallery [zFiles]="files()" zShowSearch zServerSearch [zMode]="'list'" (zSearchChange)="onServerSearch($event)" />

<!-- Fixed-width tiles instead of column count -->
<z-gallery [zFiles]="files()" [zItemWidth]="'10rem'" />

<!-- Loading state -->
<z-gallery [zFiles]="[]" [zLoading]="isFetching()" [zSkeletonItems]="8" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled file grid, no preview/selection/skeleton support -->
<div class="grid grid-cols-4 gap-4">
  <div *ngFor="let f of files">
    <img [src]="f.thumbnailUrl" />
    {{ f.name }}
  </div>
</div>

<!-- WRONG: invented prop, there is no zLayout -->
<z-gallery [zFiles]="files" zLayout="masonry" />

<!-- WRONG: one-way binding on zSelectedFiles drops selection state on parent re-render -->
<z-gallery [zFiles]="files" zSelectable [zSelectedFiles]="selected" />
```
