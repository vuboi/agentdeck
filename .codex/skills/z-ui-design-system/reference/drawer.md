# Drawer — `z-drawer`

Selector: `z-drawer`. A panel that slides in from an edge of the viewport, with an optional header
(title/description/close button), a scrollable body, and an optional OK/Cancel footer. Two usage modes:
declarative **template mode** (`<z-drawer>` in your template, driven by `[(zVisible)]`) or imperative
**service mode** (`ZDrawerService.create(...)`, for dynamically-loaded content/components). Never build a
slide-over panel by hand with `position: fixed` + a manual backdrop.

In template mode the footer's OK/Cancel buttons are **hidden unless you pass `zOkText`/`zCancelText`
explicitly** (their default is `null`, and `null` means "don't render this button") — a bare `<z-drawer>`
with no `zOkText`/`zCancelText` shows a footer-less panel, so always set them if you want the default buttons:

```html
<button z-button (click)="drawerOpen.set(true)">Open</button>

<z-drawer
  [(zVisible)]="drawerOpen"
  zTitle="Edit item"
  zPlacement="right"
  zOkText="Save"
  zCancelText="Cancel"
  (zOnOk)="save()"
>
  <p>Drawer body content...</p>
</z-drawer>
```

```ts
// Service mode
const drawerRef = this._drawerService.create({
  zContent: MyFormComponent,
  zTitle: 'Edit item',
  zData: { id: 1 },
});
drawerRef.zAfterClose.subscribe(result => { ... });
```

## Inputs (template mode — `ZDrawerComponent`)

| Input             | Type                                     | Default                                           | Notes                                                                                                                                                                                                                           |
| ----------------- | ---------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`           | `ClassValue`                             | `''`                                              | Extra classes merged with variant classes                                                                                                                                                                                       |
| `zBodyClass`      | `ClassValue`                             | `''`                                              | Extra classes for the body/content wrapper                                                                                                                                                                                      |
| `zVisible`        | `model<boolean>`                         | `false`                                           | Two-way bindable open state — use `[(zVisible)]`                                                                                                                                                                                |
| `zTitle`          | `string \| TemplateRef<unknown>`         | `undefined`                                       | Header title; supports plain text, HTML string, or a template                                                                                                                                                                   |
| `zDescription`    | `string \| TemplateRef<unknown>`         | `undefined`                                       | Subtitle text under the title                                                                                                                                                                                                   |
| `zWidth`          | `string`                                 | `'400px'`                                         | Width for `left`/`right` placement                                                                                                                                                                                              |
| `zHeight`         | `string`                                 | `'400px'`                                         | Height for `top`/`bottom` placement                                                                                                                                                                                             |
| `zPlacement`      | `'left' \| 'right' \| 'top' \| 'bottom'` | `'right'`                                         | Edge the drawer slides from                                                                                                                                                                                                     |
| `zClosable`       | `boolean`                                | `true`                                            | Show the header close (X) button                                                                                                                                                                                                |
| `zMaskClosable`   | `boolean`                                | `true`                                            | Clicking the backdrop closes the drawer                                                                                                                                                                                         |
| `zDisableShadow`  | `boolean`                                | `false`                                           | Disable header/footer scroll shadows                                                                                                                                                                                            |
| `zHeaderBorder`   | `boolean`                                | `false`                                           | Force a border under the header                                                                                                                                                                                                 |
| `zFooterBorder`   | `boolean`                                | `false`                                           | Force a border above the footer                                                                                                                                                                                                 |
| `zHideFooter`     | `boolean`                                | `false`                                           | Hide the default OK/Cancel footer entirely                                                                                                                                                                                      |
| `zHideHeader`     | `boolean`                                | `false`                                           | Hide the header entirely                                                                                                                                                                                                        |
| `zOkText`         | `string \| null`                         | `null`                                            | Text for the OK footer button. **In template mode the button only renders when this is non-null** — pass an explicit string to show it (falls back to i18n "OK" only when you pass `undefined`-like via service mode, not here) |
| `zCancelText`     | `string \| null`                         | `null`                                            | Text for the Cancel footer button — same non-null-to-show rule as `zOkText`                                                                                                                                                     |
| `zOkDestructive`  | `boolean`                                | `false`                                           | Render OK button as `destructive` style                                                                                                                                                                                         |
| `zOkDisabled`     | `boolean`                                | `false`                                           | Disable the OK button                                                                                                                                                                                                           |
| `zLoading`        | `boolean`                                | `false`                                           | Shows spinner on OK button, blocks OK/Cancel clicks                                                                                                                                                                             |
| `zOverlay`        | `'dark' \| 'blur'`                       | platform default                                  | Backdrop style                                                                                                                                                                                                                  |
| `zShadow`         | `boolean`                                | `false`                                           | Force header/footer shadow regardless of scroll position                                                                                                                                                                        |
| `zShape`          | `'rounded' \| 'square'`                  | `'rounded'`                                       | Corner radius on the inner edge                                                                                                                                                                                                 |
| `zContentLoading` | `boolean`                                | `false`                                           | Shows a skeleton placeholder instead of content                                                                                                                                                                                 |
| `zSkeletonRows`   | `number \| null`                         | `null` (auto: 6 for left/right, 3 for top/bottom) | Skeleton row count while `zContentLoading`                                                                                                                                                                                      |
| `zExpand`         | `boolean`                                | `false`                                           | Enables a drag handle to resize the drawer from its outer edge                                                                                                                                                                  |
| `zScrollMode`     | `'drawer' \| 'inner'`                    | `'drawer'`                                        | Who owns vertical scroll: the drawer viewport, or content decides                                                                                                                                                               |

## Outputs

| Output        | Payload       | Fires when                                                                     |
| ------------- | ------------- | ------------------------------------------------------------------------------ |
| `zOnOk`       | `void`        | OK button clicked (not while `zLoading`)                                       |
| `zOnCancel`   | `void`        | Cancel button, header close button, or mask click (respecting `zMaskClosable`) |
| `zAfterClose` | `void`        | Close animation finished and the drawer is fully removed                       |
| `zScrollbar`  | `NgScrollbar` | The internal `ngx-scrollbar` instance becomes available                        |

## Content projection slots (directives)

| Directive selector                        | Purpose                          |
| ----------------------------------------- | -------------------------------- |
| `[z-drawer-header]` / `[zDrawerHeader]`   | Replace the entire header markup |
| `[z-drawer-content]` / `[zDrawerContent]` | Replace the body content region  |
| `[z-drawer-footer]` / `[zDrawerFooter]`   | Replace the entire footer markup |
| (default, unselected)                     | Body content in template mode    |

## `zPlacement` — no size table, layout only

`z-drawer` has no `zSize` variant; sizing is controlled directly via `zWidth`/`zHeight` per `zPlacement`. From
`z-drawer.variants.ts`:

| `zPlacement`      | Position classes                      | Sized by  |
| ----------------- | ------------------------------------- | --------- |
| `left`            | `left-0 top-0 h-full max-w-[90vw]`    | `zWidth`  |
| `right` (default) | `right-0 top-0 h-full max-w-[90vw]`   | `zWidth`  |
| `top`             | `top-0 left-0 w-full max-h-[90vh]`    | `zHeight` |
| `bottom`          | `bottom-0 left-0 w-full max-h-[90vh]` | `zHeight` |

`zShape="rounded"` adds a radius on the inner corner only (e.g. `rounded-l-lg` for `right` placement);
`zShape="square"` adds none.

## Service mode — `ZDrawerService`

Inject `ZDrawerService` and call `.create<T, U>(options: ZDrawerOptions<T, U>)`, which returns a `ZDrawerRef<T>`.
`ZDrawerOptions` mirrors most template inputs (`zTitle`, `zWidth`, `zPlacement`, `zOkText`, etc.) plus
service-only fields: `zContent` (component `Type`, `TemplateRef`, or string), `zData` (injected via
`Z_DRAWER_DATA` into the content component), `zDefer` (an `Observable` to await before rendering, shows a
skeleton until it resolves), `zOnOk`/`zOnCancel` (`EventEmitter` or callback returning `false` to prevent
close), `zLoading`/`zContentLoading` (`WritableSignal<boolean>`), `zOkIcon`/`zCancelIcon`.

`ZDrawerRef<T>` members: `componentInstance: T | null`, `loading: Signal<boolean>`, `zAfterClose: EventEmitter<R | undefined>`,
`setLoading(value)`, `close(result?)`.

Content components can inject `ZDrawerRef` and `Z_DRAWER_DATA` directly to close themselves or read passed data.

## Examples

```html
<!-- Simple right-side drawer with an explicit OK/Cancel footer -->
<z-drawer
  [(zVisible)]="open"
  zTitle="Create user"
  zOkText="Create"
  zCancelText="Cancel"
  (zOnOk)="onCreate()"
  (zOnCancel)="onCancel()"
>
  <z-input zLabel="Name" [(ngModel)]="name" />
</z-drawer>

<!-- Bottom sheet, resizable, no mask-close, no footer -->
<z-drawer
  [(zVisible)]="sheetOpen"
  zPlacement="bottom"
  zHeight="320px"
  [zExpand]="true"
  [zMaskClosable]="false"
  [zHideFooter]="true"
  zTitle="Filters"
>
  ...
</z-drawer>

<!-- Async save with loading state -->
<z-drawer [(zVisible)]="open" zTitle="Edit" zOkText="Save" [zLoading]="isSaving()" (zOnOk)="save()">...</z-drawer>

<!-- Custom footer -->
<z-drawer [(zVisible)]="open" zTitle="Details">
  <p>Body</p>
  <div z-drawer-footer class="flex justify-end gap-2">
    <button z-button zType="outline" (click)="open.set(false)">Close</button>
  </div>
</z-drawer>
```

```ts
// Service mode with a component and injected data
const ref = this._drawerService.create<UserFormComponent, { userId: number }>({
  zContent: UserFormComponent,
  zTitle: 'Edit user',
  zData: { userId: 42 },
  zOkText: 'Save',
  zOnOk: (instance, drawerRef) => {
    if (!instance.form.valid) return false; // keep drawer open
    return { saved: true };
  },
});
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled slide-over, no focus trap, no animation, no z-index management -->
<div class="fixed top-0 right-0 h-full w-96 bg-white shadow-lg" [class.hidden]="!open">...</div>

<!-- WRONG: invented input, there is no zSize on z-drawer -->
<z-drawer zSize="lg">...</z-drawer>

<!-- WRONG: guessing the visibility API is a plain @Input -->
<z-drawer [zVisible]="open" (zVisibleChange)="open = $event">...</z-drawer>
```

```html
<!-- RIGHT: use the model() two-way binding -->
<z-drawer [(zVisible)]="open">...</z-drawer>
```
