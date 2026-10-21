# Command — `z-command`

Selector: `z-command`. A `⌘K`-style command palette dialog: a search input filtering grouped, keyboard-navigable
items, rendered in a global overlay. It manages its own overlay lifecycle (open/close animation, backdrop,
focus, scroll lock) — never build a search dialog by hand with a raw `<div class="fixed inset-0">`.

`exportAs: zCommand`

```html
<z-command [(zVisible)]="commandVisible" [zGroups]="commandGroups()" (zSelect)="onCommandSelect($event)" />
```

```ts
protected readonly commandVisible = signal(false);
protected readonly commandGroups = computed<ZCommandGroup[]>(() => [
  {
    heading: 'Actions',
    items: [{ id: 'theme-dark', label: 'Switch to Dark Theme', icon: 'lucideMoon' }],
  },
]);

protected onCommandSelect(item: ZCommandItem): void {
  // handle by item.id / item.category / item.data
}
```

No CVA variants file exists for this component (no `z-command.variants.ts`) — there is no `zType`/`zSize` input
and no size/variant table to document. Don't invent one.

## Inputs

| Input          | Type               | Default                                                                       | Notes                                                                                                                      |
| -------------- | ------------------ | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`       | `''`                                                                          | Extra classes merged with the dialog's panel classes                                                                       |
| `zVisible`     | `model<boolean>`   | `false`                                                                       | Two-way bindable open state — use `[(zVisible)]`; toggle it (e.g. on a global `⌘K`/`Ctrl+K` keydown handler) to open/close |
| `zPlaceholder` | `string`           | `'i18n_z_ui_command_search'`                                                  | Search input placeholder, run through `TranslatePipe`                                                                      |
| `zGroups`      | `ZCommandGroup[]`  | `[]`                                                                          | The groups/items to render and search over                                                                                 |
| `zOverlay`     | `'dark' \| 'blur'` | browser-detected (`getOverlayType()`; `'dark'` on Chrome, `'blur'` elsewhere) | Backdrop style                                                                                                             |

## Outputs

| Output        | Payload        | Notes                                                                                      |
| ------------- | -------------- | ------------------------------------------------------------------------------------------ |
| `zSelect`     | `ZCommandItem` | Emitted when an item is chosen (click or Enter); the dialog also closes itself right after |
| `zAfterClose` | `void`         | Emitted once the close animation finishes and the overlay is fully disposed                |

## Data shapes

```ts
interface ZCommandItem<T = any> {
  id: string;
  label: string;
  icon?: ZIcon;
  shortcut?: string; // e.g. '⌘G', rendered as a <kbd>
  category?: string; // your own grouping/routing key, read back in the zSelect handler
  data?: T; // arbitrary payload, e.g. { path: '/settings' }
}

interface ZCommandGroup {
  heading: string;
  items: ZCommandItem[];
}
```

Both types are exported from `@shival99/z-ui/components/z-command` (aliased in this repo's demo app as
`@core-ui/z-command`).

## Behavior notes

- Search matches against `item.label` and the group's `heading`, case-insensitive.
- Arrow Up/Down moves selection (wraps around), Enter selects, Escape closes — all built in, wired via
  `(keydown)` on the search input.
- Groups with zero matching items after filtering are hidden entirely.
- The component has no imperative `.open()`/`.close()` requirement to use it — driving `zVisible` (e.g. via a
  signal and `[(zVisible)]`) is the primary API — but a template reference (`#cmd="zCommand"`) also exposes
  `open()` and `close()` methods if you need to call them from code without a bound signal.

## Examples

```html
<!-- Basic palette bound to a signal, opened via a global Ctrl+K/Cmd+K handler -->
<z-command [(zVisible)]="commandVisible" [zGroups]="commandGroups()" (zSelect)="onCommandSelect($event)" />
```

```ts
@HostListener('window:keydown', ['$event'])
onWindowKeyDown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    this.commandVisible.update(visible => !visible);
  }
}
```

```html
<!-- Custom placeholder + blurred backdrop -->
<z-command
  [(zVisible)]="commandVisible"
  [zGroups]="commandGroups()"
  zPlaceholder="Search commands…"
  zOverlay="blur"
  (zSelect)="onCommandSelect($event)"
/>
```

```ts
// Grouped items with icons, shortcuts, and a routing payload
protected readonly commandGroups = computed<ZCommandGroup[]>(() => [
  {
    heading: 'Components',
    items: this.categories.flatMap(cat =>
      cat.items.map(item => ({
        id: item.id,
        label: item.name,
        icon: item.icon,
        category: 'navigation',
        data: { path: item.path },
      }))
    ),
  },
  {
    heading: 'Links',
    items: [{ id: 'github', label: 'Go to GitHub Repository', icon: 'brandGithub', shortcut: '⌘G' }],
  },
]);

protected onCommandSelect(item: ZCommandItem): void {
  if (item.category === 'navigation' && item.data?.path) {
    void this._router.navigateByUrl(item.data.path);
  }
}
```

```html
<!-- Reacting to the close animation finishing -->
<z-command [(zVisible)]="commandVisible" [zGroups]="commandGroups()" (zAfterClose)="onCommandClosed()" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled search dialog, no overlay/focus/keyboard-nav handling -->
<div class="fixed inset-0 flex items-start justify-center bg-black/50 pt-24">
  <div class="w-full max-w-lg rounded-xl bg-white">
    <input type="text" placeholder="Search..." />
  </div>
</div>

<!-- WRONG: invented size/variant props — z-command has no CVA variants file -->
<z-command zSize="lg" zType="minimal" [zGroups]="commandGroups()" />

<!-- WRONG: passing a flat item list instead of grouped ZCommandGroup[] -->
<z-command [zGroups]="flatItemsArray" />

<!-- RIGHT: groups with a heading, driven by a model signal -->
<z-command [(zVisible)]="commandVisible" [zGroups]="[{ heading: 'Actions', items: flatItemsArray }]" />
```
