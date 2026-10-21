# Overlay — `ZOverlayService`

Not a component — there is no `z-overlay` selector, no template, no CVA variants, no `input()`/`output()`.
It's the low-level positioning/mounting engine (floating-ui + native `popover="manual"`, replacing CDK
overlay/portal as of commit `21eabbbe`) that other z-ui overlay components build on: `z-modal`, `z-drawer`,
`z-popover`, `z-tooltip`, `z-toast`, `z-gallery-preview`, `z-command`. Application code building a normal UI
should reach for one of those higher-level components, not this service, the same way you wouldn't reach for
CDK's `Overlay` directly when `MatDialog` already does what you need.

It is still a clean, directly injectable public API (exported from `@shival99/z-ui/components/z-overlay`), so
use it directly only when building a **new** custom anchored/global overlay primitive that isn't already
covered by an existing z-ui component — e.g. a bespoke context menu or a one-off floating panel.

```ts
import { ZOverlayService, type ZOverlayRef } from '@shival99/z-ui/components/z-overlay';

export class MyPanelService {
  private readonly _overlay = inject(ZOverlayService);

  open(): ZOverlayRef {
    const overlayRef = this._overlay.createGlobal({ hasBackdrop: true, scrollLock: true });
    overlayRef.attach(MyPanelComponent);
    return overlayRef;
  }
}
```

## `ZOverlayService` methods

| Method           | Params                            | Returns       | Purpose                                                                                                                                                                              |
| ---------------- | --------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `createAnchored` | `config?: ZOverlayAnchoredConfig` | `ZOverlayRef` | Trigger-anchored overlay (tooltip/popover-style) — host is `popover="manual"`, plus a `.z-overlay-pane` child that Floating UI positions with `left`/`top`                           |
| `createGlobal`   | `config: ZOverlayGlobalConfig`    | `ZOverlayRef` | Full-screen overlay (modal/drawer) — the attached component self-positions (already `fixed`), so the host itself is the overlay element; optionally adds a backdrop and locks scroll |

### `ZOverlayAnchoredConfig`

| Field        | Type     | Default     | Purpose                                                                  |
| ------------ | -------- | ----------- | ------------------------------------------------------------------------ |
| `panelClass` | `string` | `undefined` | Extra class(es) (space-separated) added to the `.z-overlay-pane` element |

### `ZOverlayGlobalConfig`

| Field           | Type                 | Default      | Purpose                                                                                                                    |
| --------------- | -------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `hasBackdrop`   | `boolean`            | — (required) | Whether to create and append a `.z-overlay-backdrop` element                                                               |
| `backdropClass` | `string`             | `undefined`  | Extra class(es) added to the backdrop element                                                                              |
| `panelClass`    | `string`             | `undefined`  | Extra class(es) added to the host element                                                                                  |
| `scrollLock`    | `boolean`            | `undefined`  | Locks page scroll (via `ZScrollLockService`) while this overlay is open, ref-counted so nested overlays don't unlock early |
| `size`          | `ZOverlaySizeConfig` | `undefined`  | Applied immediately via `overlayRef.updateSize()`                                                                          |

## `ZOverlayRef` — the handle returned by both `create*` methods

| Member                                                      | Type                                                                                                            | Purpose                                                                                                                                        |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `hostElement`                                               | `HTMLElement` (readonly)                                                                                        | The `popover="manual"` host node                                                                                                               |
| `overlayElement`                                            | `HTMLElement` (readonly)                                                                                        | The node Floating UI positions (`.z-overlay-pane` for anchored; same as `hostElement` for global)                                              |
| `backdropElement`                                           | `HTMLElement \| null` (readonly)                                                                                | The backdrop node, if `hasBackdrop` was set                                                                                                    |
| `hasAttached()`                                             | `(): boolean`                                                                                                   | Whether a component or template is currently attached                                                                                          |
| `attach<T>(component, options?)`                            | `(component: Type<T>, options?: { injector?: Injector; viewContainerRef?: ViewContainerRef }): ComponentRef<T>` | Mounts an Angular component into `overlayElement`. Without `viewContainerRef`, uses `createComponent` + `ApplicationRef.attachView`            |
| `attachTemplate<C>(templateRef, viewContainerRef, context)` | `(templateRef: TemplateRef<C>, viewContainerRef: ViewContainerRef, context: C): EmbeddedViewRef<C>`             | Mounts a `TemplateRef` instead of a component                                                                                                  |
| `updateSize(size)`                                          | `(size: ZOverlaySizeConfig): void`                                                                              | Sets width/min-width/max-width/height/min-height/max-height inline styles on `overlayElement`                                                  |
| `backdropClick()`                                           | `(): Observable<MouseEvent>`                                                                                    | Emits on backdrop click; empty observable if there's no backdrop                                                                               |
| `outsidePointerEvents()`                                    | `(): Observable<PointerEvent>`                                                                                  | Emits on any capture-phase `pointerdown` outside `overlayElement`                                                                              |
| `dispose()`                                                 | `(): void`                                                                                                      | Destroys the attached component/view, hides the popover, removes the host from the DOM, and runs the `onDispose` callback (e.g. scroll unlock) |

## Positioning helpers (`z-overlay-floating.utils`)

Used by anchored overlays to compute and keep position in sync with the trigger via floating-ui.

| Function                     | Signature                                                                                                             | Purpose                                                                                                                                                                                                |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `computeZOverlayPlacement`   | `(config: ZFloatingPlacementConfig): Promise<ZFloatingPlacementResult>`                                               | Runs floating-ui `computePosition` with `offset`, `flip` (using `fallbackPlacements`), optional `shift` (`push`), and optional `size` middleware (`flexibleDimensions`); returns `{ x, y, placement }` |
| `startZOverlayAutoUpdate`    | `(reference: Element, floating: HTMLElement, onUpdate: () => void, options?: Partial<AutoUpdateOptions>): () => void` | Wraps floating-ui `autoUpdate`; call the returned function to stop watching                                                                                                                            |
| `computeZOverlayArrowOffset` | `(config: ZFloatingArrowOffsetConfig): number`                                                                        | Clamped centering offset for an arrow/pointer element relative to the trigger                                                                                                                          |

`ZFloatingPlacementConfig` fields: `reference: Element`, `floating: HTMLElement`, `placement: ZOverlayPlacement`,
`fallbackPlacements: ZOverlayPlacement[]`, `offset: number`, `viewportPadding?: number` (default `8`),
`push?: boolean`, `flexibleDimensions?: boolean`.

`ZOverlayPlacement` is a 12-way union: `'top' | 'top-left' | 'top-right' | 'bottom' | 'bottom-left' |
'bottom-right' | 'left' | 'left-top' | 'left-bottom' | 'right' | 'right-top' | 'right-bottom'`.

## `ZScrollLockService`

| Method     | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lock()`   | Ref-counted page scroll lock; adds `z-scroll-locked` (`overflow: hidden` + releases the scrollbar gutter so fixed overlays reach the physical window edge), then compensates the released width with `body` padding-right and publishes it as `--z-scroll-lock-gutter`. In-flow and `position: sticky` content stays put; the app's own `position: fixed` elements shift by that width unless they subtract the variable. |
| `unlock()` | Decrements the ref count; on reaching zero, restores scroll position and removes the class                                                                                                                                                                                                                                                                                                                                |

`createGlobal({ scrollLock: true, ... })` calls this for you — don't call it directly unless you're building
a global overlay outside `ZOverlayService`.

## Examples

```ts
// Anchored overlay (popover/tooltip-style), positioned against a trigger element,
// kept in sync on scroll/resize via autoUpdate.
import {
  computeZOverlayPlacement,
  startZOverlayAutoUpdate,
  ZOverlayService,
  type ZOverlayRef,
} from '@shival99/z-ui/components/z-overlay';

export class MyFlyoutDirective {
  private readonly _overlay = inject(ZOverlayService);
  private _overlayRef?: ZOverlayRef;
  private _stopAutoUpdate?: () => void;

  async open(trigger: HTMLElement): Promise<void> {
    this._overlayRef = this._overlay.createAnchored();
    const { instance } = this._overlayRef.attach(MyFlyoutContentComponent);

    const applyPosition = async () => {
      const floating = this._overlayRef!.overlayElement;
      const result = await computeZOverlayPlacement({
        reference: trigger,
        floating,
        placement: 'bottom',
        fallbackPlacements: ['top', 'bottom-left', 'bottom-right'],
        offset: 8,
        push: true,
      });
      floating.style.left = `${result.x}px`;
      floating.style.top = `${result.y}px`;
    };

    await applyPosition();
    this._stopAutoUpdate = startZOverlayAutoUpdate(
      trigger,
      this._overlayRef.overlayElement,
      () => void applyPosition()
    );
  }

  close(): void {
    this._stopAutoUpdate?.();
    this._overlayRef?.dispose();
  }
}
```

```ts
// Global overlay (modal/drawer-style), full-screen with backdrop + scroll lock.
import { ZOverlayService } from '@shival99/z-ui/components/z-overlay';

export class MyModalService {
  private readonly _overlay = inject(ZOverlayService);

  open(): void {
    const overlayRef = this._overlay.createGlobal({
      hasBackdrop: true,
      backdropClass: 'z-modal-backdrop-dark',
      scrollLock: true,
    });

    overlayRef.attach(MyModalContentComponent);

    overlayRef.backdropClick().subscribe(() => overlayRef.dispose());
  }
}
```

## Anti-patterns

```ts
// WRONG: for a normal dialog/tooltip/dropdown, hand-rolling a fixed/absolute div,
// z-index, and click-outside listener instead of using z-modal / z-popover / z-tooltip,
// which already wrap ZOverlayService with the right semantics.
const el = document.createElement('div');
el.style.position = 'fixed';
el.style.zIndex = '9999';
document.body.appendChild(el);

// WRONG: calling ZScrollLockService directly for a global overlay you built with
// ZOverlayService — createGlobal({ scrollLock: true }) already manages the ref count for you.
this._scrollLock.lock();
const overlayRef = this._overlay.createGlobal({ hasBackdrop: true });

// WRONG: invented config field — hasBackdrop is required on ZOverlayGlobalConfig, and there
// is no zIndex option on the service; z-index is handled by a separate z-index service.
this._overlay.createGlobal({ zIndex: 9999 });
```
