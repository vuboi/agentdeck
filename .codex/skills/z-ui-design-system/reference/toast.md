# Toast — `ZToastService`

Primarily a **service**, not a component you place in a template. It wraps `ngx-sonner` behind a global overlay
that auto-attaches on first use. Never build toast notifications by hand-rolling a fixed-position `<div>` stack.

```ts
import { ZToastService } from '@shival99/z-ui/components/z-toast';

@Component({ ... })
export class MyComponent {
  private readonly _toast = inject(ZToastService);

  save(): void {
    this._toast.success('Saved successfully');
  }
}
```

No template wiring is required — `create()`/`show()`/`success()`/etc. lazily attach the toaster overlay the
first time they're called. Call `provideZToast({ ... })` once in `app.config.ts` only if you want different
global defaults than the built-in ones.

```ts
// app.config.ts
providers: [
  provideZToast({ zPosition: 'top-right', zDuration: 5000, zCloseButton: true }),
],
```

## `ZToastService` methods

| Method       | Signature                                                                                       | Notes                                                                                                                             |
| ------------ | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `show`       | `(message: string, options?: ZToastOptions) => string \| number`                                | Neutral/default toast; returns the toast id                                                                                       |
| `success`    | `(message: string, options?: ZToastOptions) => string \| number`                                |                                                                                                                                   |
| `error`      | `(message: string, options?: ZToastOptions) => string \| number`                                |                                                                                                                                   |
| `warning`    | `(message: string, options?: ZToastOptions) => string \| number`                                |                                                                                                                                   |
| `info`       | `(message: string, options?: ZToastOptions) => string \| number`                                |                                                                                                                                   |
| `loading`    | `(message: string, options?: ZToastOptions) => string \| number`                                | Persistent spinner toast — dismiss it manually or replace it (e.g. via `promise`)                                                 |
| `promise`    | `<T>(promise: Promise<T>, messages: ZToastPromiseMessages<T>) => string \| number \| undefined` | Shows loading → success/error automatically based on the promise outcome                                                          |
| `custom`     | `(config: ZCustomToastConfig, options?: ZToastOptions) => string \| number`                     | Renders the richer `ZToastCustomComponent` (title/description/icon/avatar/image/progress/actions) instead of a plain message      |
| `dismiss`    | `(toastId?: string \| number) => void`                                                          | Dismiss one toast (or the most recent if omitted)                                                                                 |
| `dismissAll` | `() => void`                                                                                    | Dismiss every visible toast                                                                                                       |
| `create`     | `(config?: ZToasterConfig) => void`                                                             | Explicitly attach the toaster overlay early with a specific config (rarely needed — auto-attaches on first `show`/`success`/etc.) |
| `destroy`    | `() => void`                                                                                    | Tear down the overlay (mainly for tests / manual cleanup)                                                                         |

`message` strings are passed through the app's `ZTranslateService`, so an i18n key works directly.

## `ZToastOptions` (extends ngx-sonner's `ExternalToast`)

| Field         | Type               | Notes                                                |
| ------------- | ------------------ | ---------------------------------------------------- |
| `description` | `string`           | Secondary line under the message                     |
| `duration`    | `number` (ms)      | Overrides the global `zDuration` for this toast      |
| `id`          | `string \| number` | Explicit id, e.g. to update/replace a specific toast |

(Plus whatever `ExternalToast` from `ngx-sonner` exposes — action buttons, cancel, onDismiss, etc.)

## `ZToastPromiseMessages<T>` (for `promise()`)

| Field     | Type                                     |
| --------- | ---------------------------------------- |
| `loading` | `string`                                 |
| `success` | `string \| ((data: T) => string)`        |
| `error`   | `string \| ((error: unknown) => string)` |

## `ZCustomToastConfig` (for `custom()`)

| Field                                            | Type                                                                                            | Notes                                                                                                                                                               |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `title`                                          | `string`                                                                                        | Required                                                                                                                                                            |
| `description`                                    | `string`                                                                                        | Optional                                                                                                                                                            |
| `type`                                           | `'success' \| 'error' \| 'warning' \| 'info' \| 'avatar' \| 'image' \| 'progress' \| 'default'` | Drives the default icon/color when `icon` isn't set                                                                                                                 |
| `icon`                                           | `string`                                                                                        | Overrides the default icon for `type`                                                                                                                               |
| `avatarUrl` / `avatarOnline` / `avatarBadgeIcon` |                                                                                                 | Used when `type === 'avatar'`                                                                                                                                       |
| `imageUrl`                                       | `string`                                                                                        | Used when `type === 'image'`                                                                                                                                        |
| `progress`                                       | `number`                                                                                        | Used when `type === 'progress'`                                                                                                                                     |
| `actions`                                        | `ZCustomToastAction[]`                                                                          | `{ label, onClick: (toastId) => void, zType?: 'primary' \| 'secondary' \| 'outline' \| 'text' \| 'danger' }` — clicking an action dismisses the toast automatically |
| `closeable`                                      | `boolean`                                                                                       | Show a close (×) button                                                                                                                                             |

## `provideZToast(config?)` / `ZToasterConfig` (global defaults)

| Field            | Type                                                                                              | Default       |
| ---------------- | ------------------------------------------------------------------------------------------------- | ------------- |
| `zTheme`         | `'light' \| 'dark' \| 'system'`                                                                   | `'system'`    |
| `zPosition`      | `'top-left' \| 'top-center' \| 'top-right' \| 'bottom-left' \| 'bottom-center' \| 'bottom-right'` | `'top-right'` |
| `zRichColors`    | `boolean`                                                                                         | `true`        |
| `zExpand`        | `boolean`                                                                                         | `true`        |
| `zDuration`      | `number` (ms)                                                                                     | `2500`        |
| `zVisibleToasts` | `number`                                                                                          | `3`           |
| `zCloseButton`   | `boolean`                                                                                         | `true`        |
| `zDir`           | `'ltr' \| 'rtl' \| 'auto'`                                                                        | `'auto'`      |

## `zToastVariants` — color (used internally by `ZToastComponent`'s rendering, exported for advanced theming)

| `zVariant` | Style                   |
| ---------- | ----------------------- |
| `default`  | Neutral background/text |
| `success`  | Green                   |
| `error`    | Red                     |
| `warning`  | Amber                   |
| `info`     | Blue                    |

## Examples

```ts
// Simple feedback
this._toast.success('Profile updated');
this._toast.error('Something went wrong');

// With description and custom duration
this._toast.warning('Session expiring soon', { description: 'Save your work.', duration: 8000 });

// Async operation
this._toast.promise(this.api.saveDraft(draft), {
  loading: 'Saving draft...',
  success: 'Draft saved',
  error: err => `Failed to save: ${err}`,
});

// Rich custom toast with an action
this._toast.custom({
  title: 'New comment',
  description: 'Alex replied to your thread.',
  type: 'info',
  actions: [{ label: 'View', onClick: () => this.router.navigate(['/thread']) }],
  closeable: true,
});
```

## Anti-patterns

```ts
// WRONG: hand-rolled toast state + fixed-position div in the template
this.toastMessage = 'Saved!';
this.showToast = true;
setTimeout(() => (this.showToast = false), 3000);

// WRONG: invented method, zToast is not a real API
this._toast.zToast('Saved');

// WRONG: instantiating ZToastComponent directly in a template — it's the internal
// toaster host attached automatically by the service, not something you place yourself
```

```html
<z-toast />
```
