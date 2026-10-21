# Accordion — `z-accordion`

Selectors: `z-accordion` (container), `z-accordion-item` (one collapsible section). Never build an accordion by
hand with `<div>`s and a manual `open` signal — `z-accordion` owns open/close state for its items and handles
single-vs-multiple-open behavior for you.

```html
<z-accordion>
  <z-accordion-item zId="item-1" zTitle="Is it accessible?">
    Yes. It adheres to the WAI-ARIA design pattern.
  </z-accordion-item>
  <z-accordion-item zId="item-2" zTitle="Is it styled?">Yes, it comes with default styles.</z-accordion-item>
</z-accordion>
```

## `z-accordion` Inputs

| Input          | Type                    | Default     | Notes                                                                                                            |
| -------------- | ----------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`            | `''`        | Extra classes merged with variant classes                                                                        |
| `zType`        | see variant table below | `'default'` | Visual style, cascades to all child items                                                                        |
| `zMultiple`    | `boolean`               | `false`     | Allow more than one item open at a time                                                                          |
| `zCollapsible` | `boolean`               | `true`      | Allow the open item to be closed again (if `false`, once opened an item can't be collapsed by clicking it again) |

`z-accordion` has no outputs — open state is read per-item via `z-accordion`'s own tracking, not emitted events.
It exposes public methods for template-ref control (`#acc="..."` isn't wired via `exportAs`, so reach them
through a `viewChild` of `ZAccordionComponent`): `isItemOpen(itemId)`, `toggleItem(itemId)`, `openItem(itemId)`,
`closeItem(itemId)`, `closeAll()`.

## `z-accordion-item` Inputs

| Input       | Type         | Default      | Notes                                                                         |
| ----------- | ------------ | ------------ | ----------------------------------------------------------------------------- |
| `class`     | `ClassValue` | `''`         | Extra classes on the item root                                                |
| `zId`       | `string`     | **required** | Stable identifier used for open/close tracking — must be unique per accordion |
| `zTitle`    | `string`     | **required** | Header text                                                                   |
| `zDisabled` | `boolean`    | `false`      | Prevents toggling this item                                                   |

### Content projection

| Slot        | Selector             | Purpose                                                                  |
| ----------- | -------------------- | ------------------------------------------------------------------------ |
| Default     | (unselected content) | Panel body, shown when the item is open                                  |
| Header icon | `[zAccordionIcon]`   | Optional custom icon/element placed before the title text in the trigger |

## `zType` — pick by layout, not by guessing a class

Set once on `z-accordion`; every child `z-accordion-item` picks up matching item/trigger/content/chevron
styling automatically (full definitions in
[z-accordion.variants.ts](../../../../libs/core-ui/components/z-accordion/z-accordion.variants.ts)):

| `zType`     | Container                                         | Item separators                                                          | Chevron behavior                                         |
| ----------- | ------------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------- |
| `default`   | `border border-border rounded-md overflow-hidden` | `border-b` between items                                                 | Rotates 180° on open                                     |
| `ghost`     | no border/background                              | `border-b` between items                                                 | Rotates 180° on open                                     |
| `outline`   | `border border-border rounded-md overflow-hidden` | `border-b` between items                                                 | Rotates 180° on open                                     |
| `card`      | plain (items are individually cards)              | each item is `rounded-md`, gets `ring-1 ring-border shadow-md` when open | Icon swaps to `lucideMinus`/`lucidePlus`, no rotation    |
| `elevated`  | `space-y-2` (items are separated cards)           | each item `bg-card shadow-md`, `shadow-lg` when open                     | Chevron rotates -90°→0° on open                          |
| `icon-list` | plain                                             | `border-b` between items                                                 | Icon `lucidePlus`, rotates 45° (turns into an X) on open |
| `bordered`  | `rounded-md border border-border`                 | `border-b` between items                                                 | Rotates 180° on open                                     |
| `separated` | `space-y-2`                                       | each item is its own `border border-border rounded-md`                   | Rotates 180° on open                                     |

## Examples

```html
<!-- Only one item open at a time (default) -->
<z-accordion zType="bordered">
  <z-accordion-item zId="billing" zTitle="Billing">...</z-accordion-item>
  <z-accordion-item zId="shipping" zTitle="Shipping">...</z-accordion-item>
</z-accordion>

<!-- Multiple items can stay open, FAQ-style card look -->
<z-accordion zType="card" [zMultiple]="true">
  <z-accordion-item zId="q1" zTitle="What's your refund policy?">...</z-accordion-item>
  <z-accordion-item zId="q2" zTitle="Do you offer support?">...</z-accordion-item>
</z-accordion>

<!-- Disabled item -->
<z-accordion>
  <z-accordion-item zId="locked" zTitle="Enterprise settings" [zDisabled]="true">...</z-accordion-item>
</z-accordion>

<!-- Custom leading icon in the trigger -->
<z-accordion zType="icon-list">
  <z-accordion-item zId="profile" zTitle="Profile">
    <z-icon zAccordionIcon zType="lucideUser" zSize="16" />
    Manage your public profile.
  </z-accordion-item>
</z-accordion>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled accordion with manual state, no keyboard/ARIA support -->
<div class="rounded-md border">
  <div (click)="open = !open" class="cursor-pointer p-4">Section title</div>
  @if (open) {
  <div class="p-4">Content</div>
  }
</div>

<!-- WRONG: missing required zId — open/close tracking has nothing to key on -->
<z-accordion-item zTitle="Section">...</z-accordion-item>

<!-- WRONG: invented type, not in the variant list -->
<z-accordion zType="flat">...</z-accordion>
```
