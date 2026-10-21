# Popover — `z-popover` (component) / `[z-popover]` (directive)

Two ways to attach a popover, same feature set (both wrap the same overlay/positioning engine and content
component). Never build a floating panel by hand with `position: absolute` + manual click listeners.

**Directive form** (`[z-popover]`, `exportAs="zPopover"`) — attach to any existing element as an attribute,
content comes from a `<ng-template>` passed via `zPopoverContent`:

```html
<button z-button [z-popover]="tpl" zPosition="bottom" #pop="zPopover">Open</button>
<ng-template #tpl let-close="close">
  <div class="p-3">
    Content here
    <button z-button zType="ghost" zSize="sm" (click)="close()">Close</button>
  </div>
</ng-template>
```

**Component form** (`<z-popover>`) — wraps its projected content as the trigger, content via `zContent`:

```html
<z-popover [zContent]="tpl" zPosition="bottom">
  <button z-button>Open</button>
</z-popover>
<ng-template #tpl let-close="close">...</ng-template>
```

Prefer the directive form when you already have a trigger element (button, icon, row) — it avoids an extra
wrapper element. Use the component form only when you specifically want the `<z-popover>` host wrapper.

## Inputs (shared by both)

| Input                | Type                                                                   | Default    | Notes                                                                                                                     |
| -------------------- | ---------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------- |
| `zContent`           | `TemplateRef<unknown>` (aliased to `zPopoverContent` on the directive) | `null`     | The popover body template; receives `{ $implicit: { close }, close }` context                                             |
| `zPosition`          | `ZPopoverPosition` (see below)                                         | `'bottom'` | Preferred placement, with automatic flip/fallback                                                                         |
| `zTrigger`           | `'click' \| 'hover' \| 'manual'`                                       | `'click'`  | How the popover opens. Directive also exposes `zPopoverTrigger` (input, no default) which overrides `zTrigger` when set   |
| `zClass`             | `string`                                                               | `''`       | Extra class(es) merged onto the popover panel                                                                             |
| `zShowDelay`         | `number`                                                               | `0`        | ms delay before showing                                                                                                   |
| `zHideDelay`         | `number`                                                               | `0`        | ms delay before hiding                                                                                                    |
| `zDisabled`          | `boolean`                                                              | `false`    | Disables opening entirely                                                                                                 |
| `zOffset`            | `number`                                                               | `8`        | Distance (px) between trigger and popover                                                                                 |
| `zManualClose`       | `boolean`                                                              | `false`    | Disables the default hover-leave/blur auto-close (and outside-click, unless `zOutsideClickClose` is also set)             |
| `zOutsideClickClose` | `boolean`                                                              | `false`    | When `zManualClose` is true, still close on outside click                                                                 |
| `zScrollClose`       | `boolean`                                                              | `false`    | Close immediately on ancestor scroll instead of repositioning                                                             |
| `zSticky`            | `boolean`                                                              | `false`    | Keep popover attached even when trigger scrolls near/out of viewport edge (skips reposition once trigger leaves viewport) |
| `zShowArrow`         | `boolean`                                                              | `false`    | Render the small arrow/caret pointing at the trigger                                                                      |
| `zKeyboardSafe`      | `boolean`                                                              | `false`    | On mobile, defers repositioning while a focused input/textarea inside the popover is being obscured by the keyboard       |

Directive-only inputs:

| Input           | Type                                             | Default  | Notes                                                                                              |
| --------------- | ------------------------------------------------ | -------- | -------------------------------------------------------------------------------------------------- |
| `zPopoverWidth` | `'auto' \| 'trigger' \| number`                  | `'auto'` | `'trigger'` matches the trigger element's width; a number sets a fixed px width                    |
| `zTriggerRef`   | `HTMLElement \| ElementRef<HTMLElement> \| null` | `null`   | Anchor to a different element than the host (e.g. anchor to a child icon instead of the whole row) |

## Trigger open state

The directive updates its trigger while the popover is open with `aria-expanded="true"` and the
`.z-popover-open` class; both are cleared when it closes. Use the semantic `aria-expanded:*` utilities for the
focused/open trigger surface instead of adding state manually:

```html
<button z-button z-popover [zPopoverContent]="tpl" class="aria-expanded:bg-accent aria-expanded:text-accent-foreground">
  Open
</button>
```

This also works on non-button directive triggers such as calendar or row elements. Do not rely on `:focus` alone
for click popovers because focus can move into the panel after a selection.
Reuse the trigger's existing hover token for `aria-expanded:*` so the open state stays visually consistent, for
example `hover:bg-muted aria-expanded:bg-muted`.

The directive also watches trigger visibility and DOM ancestry while open. If a responsive class changes the trigger
to `display: none`, an ancestor hides it, or the anchored trigger is removed from the document, the overlay closes
immediately instead of remaining detached at a stale viewport position. Keep responsive/conditional triggers on the
directive host or pass the changing anchor through `zTriggerRef`; do not position a still-open overlay against a
hidden trigger.

## Outputs (shared by both)

| Output          | Payload           | Fires                                                                                                                                   |
| --------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `zShow`         | `void`            | Popover finished opening                                                                                                                |
| `zHide`         | `void`            | Popover finished closing (after exit animation)                                                                                         |
| `zControl`      | `ZPopoverControl` | Emitted once on init: `{ close(), closeImmediate(), show(), toggle(), isVisible(), updatePosition() }` — capture for imperative control |
| `zOutsideClick` | `MouseEvent`      | A click outside trigger+panel was detected (fires alongside auto-close, or standalone info when `zManualClose` suppresses the close)    |

Directive-only outputs:

| Output            | Payload            | Fires                                                                     |
| ----------------- | ------------------ | ------------------------------------------------------------------------- |
| `zHideStart`      | `void`             | Close begins (before the exit animation completes) — fires before `zHide` |
| `zPositionChange` | `ZPopoverPosition` | The resolved placement changes (e.g. after a flip due to viewport edge)   |

## `zPosition` values

`'top' | 'top-left' | 'top-right' | 'bottom' | 'bottom-left' | 'bottom-right' | 'left' | 'left-top' |
'left-bottom' | 'right' | 'right-top' | 'right-bottom'`. The engine auto-falls-back to alternate placements
and pushes within a 10px viewport padding, so treat `zPosition` as a _preference_, not a guarantee.

## Shadow and overflow

The popover shell owns a placement-aware shadow through `--z-popover-shadow`: top placements cast upward,
bottom placements cast downward, and side placements use a balanced shadow. Components that project a dropdown or
calendar surface (`z-select`, `z-autocomplete`, and `z-calendar`) keep that shadow on their inner panel with
`shadow-[var(--z-popover-shadow)]` while the trigger shell can use `shadow-none`.

The content shell intentionally does **not** use `overflow-hidden`; clipping it cuts off the inner panel's shadow.
If a custom body needs clipping, add `overflow-hidden` to an inner surface that has its own shadow boundary rather
than to the popover shell. Preserve `[border-radius:inherit]` when adding custom classes so the shell and panel stay
aligned with the active `--radius` token.

## Imperative control (`#pop="zPopover"` or `zControl`)

```html
<button z-button [z-popover]="tpl" #pop="zPopover">Open</button>
<button z-button (click)="pop.toggle()">Toggle externally</button>
```

Both forms also expose public methods directly on the component/directive instance: `show()`, `hide()`,
`hideImmediate()`, `toggle()`, `isOpen(): boolean`, `updatePosition(): void`.

## Examples

```html
<!-- Hover tooltip-like popover, closes on mouseleave -->
<span z-popover [zPopoverContent]="info" zTrigger="hover" zShowArrow="true">Hover me</span>
<ng-template #info let-close="close"><div class="p-2 text-sm">Extra detail</div></ng-template>

<!-- Manual close only, e.g. a form inside the popover -->
<button z-button [z-popover]="form" zTrigger="click" [zManualClose]="true" #formPop="zPopover">Edit</button>
<ng-template #form let-close="close">
  <div class="flex flex-col gap-2 p-3">
    <z-input zLabel="Name" [(ngModel)]="name" />
    <button z-button zType="primary" zSize="sm" (click)="save(); close()">Save</button>
  </div>
</ng-template>

<!-- Match popover width to trigger, e.g. a filter dropdown -->
<button z-button [z-popover]="filters" zPopoverWidth="trigger">Filter</button>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled floating panel, no focus trap, no flip/fallback positioning -->
<div class="relative">
  <button (click)="open = !open">Open</button>
  @if (open) {
  <div class="absolute top-full mt-2 rounded border bg-white p-3">...</div>
  }
</div>

<!-- WRONG: invented prop, there is no zAutoClose -->
<button [z-popover]="tpl" [zAutoClose]="true">Open</button>

<!-- WRONG: passing a plain string/component instead of a TemplateRef -->
<z-popover [zContent]="'Just text'">Open</z-popover>
<!-- RIGHT -->
<z-popover [zContent]="tpl">Open</z-popover>
<ng-template #tpl>Just text</ng-template>
```
