# Modal — `z-modal`

Service-driven: unlike `z-button`/`z-card`, a modal is normally opened imperatively via `ZModalService`, not
placed in a template. `<z-modal>` also exists as a declarative element (`[(zVisible)]`) for template-mode use,
but service mode (`ZModalService.create/show/info/warning/danger`) is the common path.

```ts
import { inject } from '@angular/core';
import { ZModalService } from '@shival99/z-ui/components/z-modal';

export class ProfileComponent {
  private readonly _modalService = inject(ZModalService);

  openEdit(): void {
    this._modalService.create({
      zTitle: 'Edit Profile',
      zDescription: 'Make changes to your profile here.',
      zContent: '<p class="text-muted-foreground">This is a string content.</p>',
      zOkText: 'Save changes',
      zWidth: '425px',
      zOnOk: () => {
        console.log('Form submitted!');
      },
    });
  }
}
```

Never build a modal by hand with a fixed/absolute `<div>` + manual backdrop + `document.addEventListener` for
Escape — always go through `ZModalService` (or `<z-modal>` in template mode) to inherit focus handling,
Escape-to-close, backdrop click, animation, and z-index stacking for free.

## Opening a modal — `ZModalService` methods

| Method                                         | Returns              | Notes                                                                                                 |
| ---------------------------------------------- | -------------------- | ----------------------------------------------------------------------------------------------------- |
| `create<T, U>(options: ZModalOptions<T, U>)`   | `ZModalRef<T>`       | General-purpose modal. `zContent` may be a component `Type<T>`, a `TemplateRef<T>`, or an HTML string |
| `show<U>(options: ZModalConfirmOptions<U>)`    | `ZModalRef<unknown>` | Neutral confirm dialog (question icon)                                                                |
| `info<U>(options: ZModalConfirmOptions<U>)`    | `ZModalRef<unknown>` | Info confirm dialog (info icon, blue)                                                                 |
| `warning<U>(options: ZModalConfirmOptions<U>)` | `ZModalRef<unknown>` | Warning confirm dialog (triangle icon, amber)                                                         |
| `danger<U>(options: ZModalConfirmOptions<U>)`  | `ZModalRef<unknown>` | Destructive confirm dialog (X icon, red, OK button becomes destructive)                               |

`show`/`info`/`warning`/`danger` are thin wrappers over `create` — they force a centered, icon-led layout via
`zConfirmType` and take the smaller `ZModalConfirmOptions` shape (no header/footer directive slots, no
`zDefer`/`zContentLoading`).

## `create()` options — `ZModalOptions<T, U>`

| Option                            | Type                                                                     | Default                                       | Notes                                                                                                                               |
| --------------------------------- | ------------------------------------------------------------------------ | --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `zContent`                        | `string \| TemplateRef<T> \| Type<T>`                                    | —                                             | Modal body: HTML string, template, or component class                                                                               |
| `zTitle`                          | `string \| TemplateRef<T>`                                               | —                                             | Title; plain string, HTML string, or a `TemplateRef`                                                                                |
| `zDescription`                    | `string`                                                                 | —                                             | Subtitle under the title                                                                                                            |
| `zData`                           | `U`                                                                      | —                                             | Injected into the content component via `Z_MODAL_DATA`                                                                              |
| `zDefer`                          | `Observable<U>`                                                          | —                                             | Delay showing content until this observable emits; resolved value is available via `Z_MODAL_DATA`                                   |
| `zWidth`                          | `string`                                                                 | `'500px'`                                     | Applied as `min-width` in service mode                                                                                              |
| `zOkText` / `zCancelText`         | `string \| null`                                                         | `'OK'` / `'Cancel'` (translated)              | Set to `null` to hide that button                                                                                                   |
| `zOkIcon` / `zCancelIcon`         | `ZIcon`                                                                  | —                                             | Leading icon on the OK/Cancel button                                                                                                |
| `zOkDisabled`                     | `boolean`                                                                | `false`                                       | Disable the OK button                                                                                                               |
| `zOkDestructive`                  | `boolean`                                                                | `false`                                       | Renders OK button with `zType="destructive"`                                                                                        |
| `zOnOk` / `zOnCancel`             | `EventEmitter<T> \| (instance: T, modalRef?) => false \| void \| object` | no-op                                         | Callback (or emitter) run on click. Return `false` from a function callback to prevent auto-close (e.g. while awaiting an API call) |
| `zLoading`                        | `WritableSignal<boolean>`                                                | —                                             | External signal to drive the OK button's loading state (alternative to `modalRef.setLoading()`)                                     |
| `zContentLoading`                 | `WritableSignal<boolean>`                                                | —                                             | Shows a skeleton over the body while `true`                                                                                         |
| `zSkeletonRows`                   | `number`                                                                 | `3`                                           | Skeleton row count while `zContentLoading` is true                                                                                  |
| `zClosable`                       | `boolean`                                                                | `true`                                        | Show the header close (X) button                                                                                                    |
| `zMaskClosable`                   | `boolean`                                                                | `true`                                        | Close on backdrop click                                                                                                             |
| `zHideHeader` / `zHideFooter`     | `boolean`                                                                | `false`                                       | Hide the built-in header/footer entirely                                                                                            |
| `zHeaderBorder` / `zFooterBorder` | `boolean`                                                                | `false`                                       | Show a border line below header / above footer                                                                                      |
| `zDisableShadow`                  | `boolean`                                                                | `false`                                       | Disable header/footer shadows                                                                                                       |
| `zOverlay`                        | `'dark' \| 'blur'`                                                       | auto (`'dark'` on Chrome, `'blur'` elsewhere) | Backdrop style                                                                                                                      |
| `zConfirmType`                    | `'show' \| 'info' \| 'warning' \| 'danger'`                              | —                                             | Set internally by `show`/`info`/`warning`/`danger`; don't set directly from `create()`                                              |
| `zConfirmIcon`                    | `ZIcon`                                                                  | —                                             | Icon for the confirm-mode layout                                                                                                    |
| `class`                           | `ClassValue`                                                             | —                                             | Extra classes on the modal panel                                                                                                    |
| `zBodyClass`                      | `ClassValue`                                                             | —                                             | Extra classes on the body/content region                                                                                            |
| `zViewContainerRef`               | `ViewContainerRef`                                                       | —                                             | Attach point override for the overlay                                                                                               |

`ZModalConfirmOptions<U>` (used by `show`/`info`/`warning`/`danger`) is a subset: `zTitle` (required),
`zContent?`, `zIcon?`, `zOkText?`, `zCancelText?`, `zOnOk?`, `zOnCancel?`, `zMaskClosable?`, `zWidth?`, `zData?`.

## `ZModalRef<T>` — the handle returned by `open`/`create`/`show`/etc.

| Member                       | Type                           | Notes                                                                               |
| ---------------------------- | ------------------------------ | ----------------------------------------------------------------------------------- |
| `componentInstance`          | `T \| null`                    | The attached content component instance (when `zContent` is a component `Type`)     |
| `loading`                    | `Signal<boolean>`              | Read-only signal reflecting the OK button's loading state                           |
| `zAfterClose`                | `EventEmitter<R \| undefined>` | Emits once, after the close animation finishes, with the result passed to `close()` |
| `setLoading(value: boolean)` | `void`                         | Toggle the OK button's loading/disabled state (e.g. during an async save)           |
| `close(result?: R)`          | `void`                         | Close the modal, optionally passing a result to `zAfterClose` subscribers           |

Injecting `Z_MODAL_DATA` inside a content component retrieves the value passed as `zData` (or the resolved
value of `zDefer`).

## `<z-modal>` — declarative (template) mode

Use when the modal's content is static markup already in the host template, controlled by `[(zVisible)]`,
rather than opened from a service call.

```html
<button z-button zType="primary" (click)="isEditVisible.set(true)">Edit</button>

<z-modal [(zVisible)]="isEditVisible" zTitle="Edit Profile" zWidth="425px" (zOk)="save()">
  <p>Modal body content...</p>
</z-modal>
```

### Inputs (template mode)

| Input                             | Type                             | Default                                             | Notes                                            |
| --------------------------------- | -------------------------------- | --------------------------------------------------- | ------------------------------------------------ |
| `class`                           | `ClassValue`                     | `''`                                                | Extra classes merged with variant classes        |
| `zVisible`                        | `model<boolean>`                 | `false`                                             | Two-way bindable open state — use `[(zVisible)]` |
| `zTitle`                          | `string \| TemplateRef<unknown>` | —                                                   | Title text or template                           |
| `zDescription`                    | `string \| TemplateRef<unknown>` | —                                                   | Subtitle under the title                         |
| `zWidth`                          | `string`                         | `'500px'`                                           | Modal panel width                                |
| `zBodyClass`                      | `ClassValue`                     | `''`                                                | Extra classes on the body region                 |
| `zClosable`                       | `boolean`                        | `true`                                              | Show header close (X) button                     |
| `zMaskClosable`                   | `boolean`                        | `true`                                              | Close on backdrop click                          |
| `zDisableShadow`                  | `boolean`                        | `false`                                             | Disable header/footer shadows                    |
| `zHeaderBorder` / `zFooterBorder` | `boolean`                        | `false`                                             | Show border below header / above footer          |
| `zHideHeader` / `zHideFooter`     | `boolean`                        | `false`                                             | Hide the built-in header/footer                  |
| `zOkText` / `zCancelText`         | `string \| null`                 | `null` (falls back to translated `'OK'`/`'Cancel'`) | Button text; `null` hides the button             |
| `zOkDestructive`                  | `boolean`                        | `false`                                             | Destructive OK button style                      |
| `zOkDisabled`                     | `boolean`                        | `false`                                             | Disable OK button                                |
| `zLoading`                        | `boolean`                        | `false`                                             | OK button loading state                          |
| `zContentLoading`                 | `boolean`                        | `false`                                             | Show skeleton over the body                      |
| `zSkeletonRows`                   | `number`                         | `3`                                                 | Skeleton row count                               |
| `zOverlay`                        | `'dark' \| 'blur'`               | auto                                                | Backdrop style                                   |

### Outputs (template mode)

| Output        | Payload       | Notes                                                      |
| ------------- | ------------- | ---------------------------------------------------------- |
| `zOk`         | `void`        | OK button clicked                                          |
| `zCancel`     | `void`        | Cancel/close button or backdrop clicked                    |
| `zAfterClose` | `void`        | Fires after the close animation completes                  |
| `zScrollbar`  | `NgScrollbar` | Emits the internal `ngx-scrollbar` instance once available |

`zVisible`/`zOkText`/`zCancelText`/etc. above are the template-mode inputs on `<z-modal>` itself; in service
mode the equivalent behavior is driven by `ZModalOptions` fields passed to `ZModalService`, not these inputs.

## Content projection slots (header/content/footer directives)

Override the built-in header/body/footer regions from either mode by projecting a directive-marked
`<ng-template>`:

| Slot    | Selector                                | Purpose                                                 |
| ------- | --------------------------------------- | ------------------------------------------------------- |
| Header  | `[z-modal-header]` / `[zModalHeader]`   | Replace the entire header (title + close button)        |
| Content | `[z-modal-content]` / `[zModalContent]` | Replace the body (equivalent to default `<ng-content>`) |
| Footer  | `[z-modal-footer]` / `[zModalFooter]`   | Replace the entire footer (OK/Cancel buttons)           |

```html
<z-modal [(zVisible)]="isVisible">
  <ng-template z-modal-header>
    <h4 class="font-semibold">Custom header</h4>
  </ng-template>

  <ng-template z-modal-content>
    <p>Custom body markup.</p>
  </ng-template>

  <ng-template z-modal-footer>
    <button z-button zType="outline" (click)="isVisible.set(false)">Close</button>
  </ng-template>
</z-modal>
```

## Panel styling — `zModalVariants`

`z-modal.variants.ts` defines a single fixed class list (no size/variant axis — width is controlled via
`zWidth`, not a CVA variant):

```
fixed left-[50%] top-[50%] z-50 flex flex-col w-full translate-x-[-50%]
translate-y-[-50%] border bg-background py-4 shadow-lg rounded-lg
max-w-[calc(100%-2rem)] overflow-hidden max-h-[80dvh]
```

Use `zWidth` (e.g. `'425px'`, `'640px'`) to size the dialog; use `class`/`zBodyClass` for one-off spacing
tweaks. Don't invent a `zSize="lg"` input — it doesn't exist on modal.

## Examples

```ts
// Async save with loading state and manual close
public openEditModal(): void {
  const modalRef = this._modalService.create({
    zTitle: 'Save Changes',
    zContent: '<p class="text-muted-foreground">This will show loading state.</p>',
    zOkText: 'Save',
    zOnOk: (): false => {
      modalRef.setLoading(true);
      this.api.save().subscribe(() => {
        modalRef.setLoading(false);
        modalRef.close();
      });
      return false; // prevent auto-close until the API call resolves
    },
  });
}

// Destructive confirm shorthand
public confirmDelete(): void {
  this._modalService.danger({
    zTitle: 'Delete Item',
    zContent: 'This will permanently delete the item. This action cannot be undone.',
    zOkText: 'Delete',
    zOnOk: () => this.api.delete(this.itemId),
  });
}

// Component content with typed data
this._modalService.create<UserFormComponent, { userId: string }>({
  zTitle: 'Edit User',
  zContent: UserFormComponent,
  zData: { userId: '123' },
  zWidth: '450px',
});
```

```html
<!-- Declarative mode with header/footer directive overrides -->
<z-modal [(zVisible)]="isVisible" zWidth="450px" (zAfterClose)="onClosed()">
  <ng-template z-modal-header>
    <h4 class="font-semibold">Delete account</h4>
  </ng-template>
  <p class="text-muted-foreground text-sm">This action is permanent.</p>
  <ng-template z-modal-footer>
    <button z-button zType="outline" (click)="isVisible.set(false)">Cancel</button>
    <button z-button zType="destructive" (click)="onDelete()">Delete</button>
  </ng-template>
</z-modal>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled modal, no focus trap / Escape / z-index stacking -->
<div class="fixed inset-0 flex items-center justify-center bg-black/50" *ngIf="isOpen">
  <div class="rounded-lg bg-white p-4">...</div>
</div>
```

```ts
// WRONG: guessing a size input that doesn't exist — use zWidth instead
this._modalService.create({ zSize: 'lg', zContent: '...' });

// WRONG: calling ZModalRef.close() from inside zOnOk without returning false first —
// zOnOk already auto-closes unless it returns false, so this double-closes
this._modalService.create({
  zOnOk: (instance, modalRef) => {
    modalRef.close();
  },
});

// WRONG: using ZModalComponent directly with CDK Dialog/Overlay APIs — this repo migrated
// off @angular/cdk overlay/portal to floating-ui + native Popover (commit 21eabbbe);
// always go through ZModalService or <z-modal>, never construct an Overlay yourself.
```
