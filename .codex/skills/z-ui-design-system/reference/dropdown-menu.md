# Dropdown Menu — `z-dropdown-menu`

Selector: `z-dropdown-menu`. A popover-backed menu driven by a declarative `ZDropdownMenuItem[]` array — icons,
shortcuts, dividers, loading/disabled states, and nested submenus, all without touching `z-popover` directly.
Renders a default icon+label button trigger, or wrap your own trigger element as projected content. Never
build a menu by hand with a raw `z-popover` + `<ul>`.

```html
<z-dropdown-menu
  zLabel="Actions"
  [zItems]="[
    { label: 'Edit', icon: 'lucidePencil', onClick: onEdit },
    { label: 'Delete', icon: 'lucideTrash', divide: 'before', onClick: onDelete },
  ]"
/>
```

## Inputs

| Input         | Type                                               | Default          | Notes                                                                     |
| ------------- | -------------------------------------------------- | ---------------- | ------------------------------------------------------------------------- |
| `zItems`      | `ZDropdownMenuItem[]`                              | **required**     | The menu items to render                                                  |
| `zLabel`      | `string`                                           | `undefined`      | Text label on the default button trigger (translated via `TranslatePipe`) |
| `zIcon`       | `ZIcon`                                            | `'lucideMenu'`   | Leading icon on the default button trigger                                |
| `zButtonType` | `'primary' \| 'secondary' \| 'outline' \| 'ghost'` | `'outline'`      | `zType` passed through to the default `z-button` trigger                  |
| `zButtonSize` | `ZButtonVariants['zSize']`                         | `'default'`      | `zSize` passed through to the default `z-button` trigger                  |
| `zPosition`   | `ZPopoverPosition`                                 | `'bottom-right'` | Popover placement, forwarded to `z-popover`                               |
| `zOffset`     | `number`                                           | `4`              | Popover offset in px                                                      |
| `zMinWidth`   | `number`                                           | `224`            | Menu content min-width in px                                              |
| `zMaxWidth`   | `number`                                           | `320`            | Menu content max-width in px                                              |
| `zDisabled`   | `boolean`                                          | `false`          | Disables the trigger and popover                                          |
| `zWave`       | `boolean`                                          | `true`           | Click ripple on the default button trigger                                |

## Outputs

| Output         | Payload             | Fires when                                                                   |
| -------------- | ------------------- | ---------------------------------------------------------------------------- |
| `zOnItemClick` | `ZDropdownMenuItem` | A leaf menu item (no submenu) is clicked, and it wasn't `disabled`/`loading` |

## `ZDropdownMenuItem` — the item model

There is no separate `<z-dropdown-menu-item>` component; everything is data. All fields:

| Field                         | Type                     | Notes                                                                                                        |
| ----------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------ |
| `label`                       | `string`                 | **required**, rendered through `TranslatePipe`                                                               |
| `icon`                        | `ZIcon`                  | Leading icon                                                                                                 |
| `iconSize`                    | `ZIconVariants['zSize']` | Defaults to `'14'` if omitted                                                                                |
| `shortcut`                    | `string`                 | Right-aligned shortcut text (e.g. `'⌘K'`)                                                                    |
| `class`                       | `string`                 | Extra classes on the item button                                                                             |
| `loading`                     | `boolean`                | Shows a spinning loader icon instead of `icon`, blocks click                                                 |
| `disabled`                    | `boolean`                | Disables the item, blocks click                                                                              |
| `hidden`                      | `boolean`                | Excludes the item from render (filtered out of `zItems` before display)                                      |
| `onClick`                     | `() => void`             | Called before `zOnItemClick` emits                                                                           |
| `divide`                      | `'before' \| 'after'`    | Renders a divider line before/after this item                                                                |
| `submenu`                     | `ZDropdownMenuItem[]`    | Nests a hover-triggered submenu; when set, the item becomes non-clickable itself (it just opens the submenu) |
| `submenuPosition`             | `ZPopoverPosition`       | Position of the submenu popover (default `'right'`)                                                          |
| `subMinWidth` / `subMaxWidth` | `number`                 | Override `zMinWidth`/`zMaxWidth` for this item's submenu                                                     |

No size/variant CVA table applies here — the menu surface itself has no `zSize`/`zType` input; only the
default trigger button reuses `z-button`'s size/type system via `zButtonSize`/`zButtonType`.

## Custom trigger

Any projected content becomes the trigger instead of the default button — the component detects projected
content in `ngAfterContentInit` and swaps to it automatically:

```html
<z-dropdown-menu [zItems]="items">
  <button z-button zType="ghost" zSize="xs" [zTypeIcon]="'lucideMoreVertical'"></button>
</z-dropdown-menu>
```

## Examples

```html
<!-- Default icon+label trigger -->
<z-dropdown-menu
  zLabel="Actions"
  zIcon="lucideSettings"
  [zItems]="[
    { label: 'Rename', icon: 'lucidePencil', shortcut: '⌘R', onClick: onRename },
    { label: 'Duplicate', icon: 'lucideCopy', onClick: onDuplicate },
    { label: 'Delete', icon: 'lucideTrash', divide: 'before', onClick: onDelete },
  ]"
  (zOnItemClick)="onAnyItemClick($event)"
/>

<!-- Icon-only custom trigger, submenu, loading item -->
<z-dropdown-menu
  zPosition="bottom-left"
  [zItems]="[
    { label: 'Export', icon: 'lucideDownload', submenu: [
        { label: 'CSV', onClick: exportCsv },
        { label: 'PDF', onClick: exportPdf },
      ] },
    { label: 'Syncing…', icon: 'lucideRefreshCw', loading: isSyncing() },
  ]"
>
  <button z-button zType="ghost" [zTypeIcon]="'lucideMoreHorizontal'"></button>
</z-dropdown-menu>

<!-- Disabled trigger -->
<z-dropdown-menu zLabel="Bulk actions" [zDisabled]="selection().length === 0" [zItems]="bulkItems" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled menu with a raw popover and <ul>, no keyboard/hover semantics -->
<div z-popover [zPopoverContent]="menu">Actions</div>
<ng-template #menu>
  <ul>
    <li (click)="edit()">Edit</li>
    <li (click)="delete()">Delete</li>
  </ul>
</ng-template>

<!-- WRONG: invented input, there is no zItems[].label as a separate component tag -->
<z-dropdown-menu>
  <z-dropdown-menu-item label="Edit" />
</z-dropdown-menu>

<!-- WRONG: submenu items don't take onClick at the parent level once nested -->
<z-dropdown-menu [zItems]="[{ label: 'Export', submenu: subItems, onClick: exportAll }]" />
```

```html
<!-- RIGHT: pass items as data -->
<z-dropdown-menu
  [zItems]="[{ label: 'Edit', icon: 'lucidePencil', onClick: edit }, { label: 'Delete', onClick: del }]"
/>
```
