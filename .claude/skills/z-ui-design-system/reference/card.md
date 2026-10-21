# Card — `z-card`

Selector: `z-card`. A bordered content container with an optional header (title, collapsible chevron, extra
slot) and a padded content region. Never build a card by hand with `<div class="border rounded-lg p-4">`.

```html
<z-card zTitle="Account settings">
  <p>Card body content...</p>
</z-card>
```

## Inputs

| Input          | Type                        | Default     | Notes                                                                                                       |
| -------------- | --------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                | `''`        | Extra classes merged with variant classes                                                                   |
| `zTitle`       | `string`                    | `''`        | Header title text; header only renders if `zTitle`, `zCollapsible`, or a custom title projection is present |
| `zSize`        | `'sm' \| 'default' \| 'lg'` | `'default'` | Header/content padding + title text size                                                                    |
| `zCollapsible` | `boolean`                   | `false`     | Adds a clickable header with chevron that toggles content                                                   |
| `zHiddenIcon`  | `boolean`                   | `false`     | Hide the chevron even when collapsible                                                                      |
| `zCollapsed`   | `model<boolean>`            | `false`     | Two-way bindable collapsed state — use `[(zCollapsed)]`                                                     |

## Content projection slots

| Slot         | Selector             | Purpose                                                            |
| ------------ | -------------------- | ------------------------------------------------------------------ |
| Default      | (unselected content) | Card body                                                          |
| Custom title | `[zCardTitle]`       | Replace the plain-text title with rich markup (e.g. title + badge) |
| Header extra | `[zCardExtra]`       | Right-aligned header content (e.g. an action button)               |

## `zSize` — padding + title scale

| `zSize`   | Header padding | Content padding | Title text  |
| --------- | -------------- | --------------- | ----------- |
| `sm`      | `px-3 py-2`    | `p-3`           | `text-sm`   |
| `default` | `px-4 py-3`    | `p-4`           | `text-base` |
| `lg`      | `px-5 py-4`    | `p-5`           | `text-lg`   |

## Examples

```html
<!-- Simple card -->
<z-card zTitle="Profile">
  <p class="text-muted-foreground text-sm">Manage your profile information.</p>
</z-card>

<!-- Collapsible -->
<z-card zTitle="Advanced settings" [zCollapsible]="true" [(zCollapsed)]="advancedCollapsed">...</z-card>

<!-- Header action via zCardExtra -->
<z-card zTitle="Team members">
  <button zCardExtra z-button zType="ghost" zSize="sm" [zTypeIcon]="'lucidePlus'"></button>
  <ul>
    ...
  </ul>
</z-card>

<!-- Dense card in a compact dashboard grid -->
<z-card zTitle="Stats" zSize="sm">...</z-card>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled card, no theme tokens, no consistent padding/shadow scale -->
<div class="rounded-lg border border-gray-200 p-4 shadow">
  <h3 class="mb-2 font-semibold">Profile</h3>
  ...
</div>

<!-- WRONG: invented prop -->
<z-card zElevation="2">...</z-card>
```
