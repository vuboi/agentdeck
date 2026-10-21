# Tags — `z-tags`

Selector: `z-tags`. Renders a list of tags/chips from a `ZTagItem[]` array, with optional closable and
"add new tag" interactions. Never hand-roll tag chips with `<span class="rounded px-2 ...">`.

```html
<z-tags [zTags]="tags" [zClosable]="true" (zTagClose)="onRemove($event)" />
```

```ts
protected readonly tags: ZTagItem[] = [
  { label: 'Frontend', value: 'frontend', color: 'primary' },
  { label: 'Bug', value: 'bug', color: 'danger' },
];
```

There is also a standalone attribute directive, `z-tags` on a single element (selector `[z-tags]`, `exportAs="zTags"`), for styling one hand-placed tag element without the list/add/close behavior — e.g. inside a table cell renderer.

## `ZTagItem` shape (each entry of `zTags`)

| Field      | Type               | Notes                                                    |
| ---------- | ------------------ | -------------------------------------------------------- |
| `label`    | `string`           | Passed through `TranslatePipe`                           |
| `value`    | `string \| number` | Unique id (used for tracking and the random-color cache) |
| `color`    | `ZTagColor`        | Optional per-tag override of `zColor`                    |
| `closable` | `boolean`          | Optional per-tag override of `zClosable`                 |
| `disabled` | `boolean`          | Optional per-tag override of `zDisabled`                 |

## Inputs (`ZTagsComponent`)

| Input             | Type                        | Default     | Notes                                                                                                                                  |
| ----------------- | --------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `class`           | `ClassValue`                | `''`        | Extra classes on the host                                                                                                              |
| `zTags`           | `ZTagItem[]`                | `[]`        | The tags to render                                                                                                                     |
| `zSize`           | `'sm' \| 'default' \| 'lg'` | `'default'` | Chip padding/gap                                                                                                                       |
| `zColor`          | `ZTagColor`                 | `'default'` | Default color for tags that don't set their own `color`                                                                                |
| `zClosable`       | `boolean`                   | `false`     | Default closable state for tags that don't set their own `closable`                                                                    |
| `zDisabled`       | `boolean`                   | `false`     | Disables all tags (close button + add control)                                                                                         |
| `zAddable`        | `boolean`                   | `false`     | Shows a dashed "add tag" button/input at the end of the list; after Enter, the input stays ready for the next tag until blur or Escape |
| `zAddText`        | `string`                    | `''`        | Label for the add button; falls back to the i18n key `i18n_z_ui_tags_add`                                                              |
| `zAddPlaceholder` | `string`                    | `''`        | Placeholder for the add input; falls back to `i18n_z_ui_tags_placeholder`                                                              |
| `zRandomColor`    | `boolean`                   | `false`     | Assign a stable random color per tag `value` instead of using `zColor`/`color`                                                         |
| `zTagClass`       | `ClassValue`                | `''`        | Extra classes merged onto each tag chip                                                                                                |
| `zLabelClass`     | `ClassValue`                | `''`        | Extra classes merged onto each tag's label `<span>`                                                                                    |
| `zMaxWidth`       | `string`                    | `''`        | CSS max-width applied to each tag label (adds `truncate`)                                                                              |

## Inputs (`[z-tags]` directive, single-tag attribute form)

| Input       | Type                        | Default     | Notes                            |
| ----------- | --------------------------- | ----------- | -------------------------------- |
| `zSize`     | `'sm' \| 'default' \| 'lg'` | `'default'` | Same size scale as the component |
| `zColor`    | `ZTagColor`                 | `'default'` | Same color scale                 |
| `zDisabled` | `boolean`                   | `false`     | Disabled visual state            |
| `zClass`    | `ClassValue`                | `''`        | Extra classes                    |

## Outputs

| Output      | Payload                        | Fires when                                                  |
| ----------- | ------------------------------ | ----------------------------------------------------------- |
| `zTagClose` | `ZTagItem`                     | The close (×) button on a closable tag is clicked           |
| `zTagAdd`   | `string` (trimmed input value) | Enter is pressed or the add input blurs with non-empty text |

## `zColor` — semantic palette

| `zColor`  | Use for                |
| --------- | ---------------------- |
| `default` | Neutral/uncategorized  |
| `primary` | Primary/brand category |
| `success` | Positive status        |
| `warning` | Caution status         |
| `danger`  | Negative/error status  |
| `info`    | Informational status   |

## `zSize` — chip padding

| `zSize`   | Classes                       | Close-icon size |
| --------- | ----------------------------- | --------------- |
| `sm`      | `px-1.5 py-0.5 text-xs gap-1` | `10`            |
| `default` | `px-2 py-1 text-xs gap-1.5`   | `12`            |
| `lg`      | `px-2.5 py-1.5 text-sm gap-2` | `14`            |

## Examples

```html
<!-- Read-only category tags -->
<z-tags [zTags]="categories" />

<!-- Closable, single shared color -->
<z-tags [zTags]="skills" zColor="primary" [zClosable]="true" (zTagClose)="removeSkill($event)" />

<!-- Addable list of free-form labels -->
<z-tags
  [zTags]="labels"
  [zAddable]="true"
  [zClosable]="true"
  (zTagAdd)="addLabel($event)"
  (zTagClose)="removeLabel($event)"
/>

<!-- Per-tag colors from data, random fallback color for uncategorized ones -->
<z-tags [zTags]="items" [zRandomColor]="true" />

<!-- Single tag styled via the attribute directive, e.g. inside a table cell -->
<span z-tags zColor="success" zSize="sm">Active</span>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled chip, no color scale, no i18n -->
<span class="inline-flex items-center gap-1 rounded bg-gray-100 px-2 py-1 text-xs">Frontend</span>

<!-- WRONG: invented input, zVariant is not a real prop -->
<z-tags [zTags]="tags" zVariant="outline" />

<!-- WRONG: passing plain strings instead of ZTagItem[] -->
<z-tags [zTags]="['frontend', 'bug']" />
```
