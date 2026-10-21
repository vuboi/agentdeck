# Tooltip — `z-tooltip`

Selectors: `[z-tooltip]`, `[zTooltip]` (attribute directive, `exportAs="zTooltip"`). Apply it to any element to
attach a floating-ui-positioned tooltip. Never build a tooltip with a `title` attribute or a hand-rolled
absolutely-positioned `<div>`.

```html
<button z-button zType="ghost" zSize="sm" z-tooltip zContent="Delete this item" zPosition="top">
  <z-icon zType="lucideTrash" zSize="16" />
</button>
```

By default the tooltip only shows when its trigger element's content is actually truncated/overflowing
(`zAutoDetect`) — set `[zAlwaysShow]="true"` if you want it to show unconditionally (e.g. as a plain
hint tooltip on an icon button, not truncated text).

## Inputs

| Input                          | Type                             | Default     | Notes                                                                                                                             |
| ------------------------------ | -------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `zContent`                     | `string \| TemplateRef<unknown>` | `''`        | Tooltip body; string content is passed through `TranslatePipe`                                                                    |
| `zPosition`                    | `ZTooltipPosition` (see below)   | `'top'`     | Placement relative to the trigger                                                                                                 |
| `zTooltipPosition`             | `ZTooltipPosition`               | `undefined` | Alias that overrides `zPosition` when set (useful when the host also binds `zPosition` for something else)                        |
| `zTrigger`                     | `'hover' \| 'click'`             | `'hover'`   | How the tooltip opens                                                                                                             |
| `zTooltipTrigger`              | `'hover' \| 'click'`             | `undefined` | Alias overriding `zTrigger`                                                                                                       |
| `zType` (alias `zTooltipType`) | see color table below            | `'default'` | Visual style                                                                                                                      |
| `zSize` (alias `zTooltipSize`) | `'sm' \| 'default' \| 'lg'`      | `'default'` | Text size                                                                                                                         |
| `zClass`                       | `string`                         | `''`        | Extra classes merged onto the tooltip panel                                                                                       |
| `zTooltipClass`                | `string`                         | `undefined` | Alias overriding `zClass`                                                                                                         |
| `zShowDelay`                   | `number` (ms)                    | `150`       | Delay before showing                                                                                                              |
| `zTooltipShowDelay`            | `number`                         | `undefined` | Alias overriding `zShowDelay`                                                                                                     |
| `zHideDelay`                   | `number` (ms)                    | `0`         | Delay before hiding                                                                                                               |
| `zTooltipHideDelay`            | `number`                         | `undefined` | Alias overriding `zHideDelay`                                                                                                     |
| `zArrow`                       | `boolean`                        | `true`      | Show the pointer arrow                                                                                                            |
| `zTooltipArrow`                | `boolean`                        | `undefined` | Alias overriding `zArrow`                                                                                                         |
| `zDisabled`                    | `boolean`                        | `false`     | Disable the tooltip entirely                                                                                                      |
| `zTooltipDisabled`             | `boolean`                        | `undefined` | Alias overriding `zDisabled`                                                                                                      |
| `zOffset`                      | `number` (px)                    | `8`         | Distance from the trigger                                                                                                         |
| `zTooltipOffset`               | `number`                         | `undefined` | Alias overriding `zOffset`                                                                                                        |
| `zAutoDetect`                  | `boolean`                        | `true`      | Only show when the trigger's text is actually truncated/overflowing                                                               |
| `zTriggerElement`              | `HTMLElement \| null`            | `null`      | Use a different element than the host as the anchor/trigger (e.g. for tooltips declared inside a template but anchored elsewhere) |
| `zAlwaysShow`                  | `boolean`                        | `false`     | Bypass `zAutoDetect`'s truncation check and always show on trigger                                                                |
| `zMaxWidth`                    | `string`                         | `'250px'`   | CSS max-width of the tooltip panel                                                                                                |

Each aliased pair exists so you can use the plain name (`zPosition`) on a bare host, or the `zTooltip*`-prefixed
name when the same host already has a same-named input from another directive (e.g. `z-popover` also defines
`zPosition`).

## Outputs

| Output  | Payload | Fires when                                     |
| ------- | ------- | ---------------------------------------------- |
| `zShow` | `void`  | Tooltip becomes visible                        |
| `zHide` | `void`  | Tooltip finishes hiding (after exit animation) |

## Public methods (via `exportAs="zTooltip"` template ref)

| Method     | Notes                                                                          |
| ---------- | ------------------------------------------------------------------------------ |
| `show()`   | Programmatically show (still respects `zDisabled`/`zAutoDetect`/`zAlwaysShow`) |
| `hide()`   | Programmatically hide                                                          |
| `toggle()` | Toggle current visibility                                                      |

## `zType` — color

| `zType`   | Style                                                    |
| --------- | -------------------------------------------------------- |
| `default` | `bg-foreground text-background` (theme-aware dark/light) |
| `dark`    | Fixed dark slate background                              |
| `light`   | Fixed white background                                   |
| `primary` | `bg-primary`                                             |
| `success` | Green                                                    |
| `warning` | Amber                                                    |
| `error`   | Red                                                      |
| `info`    | Blue                                                     |

## `zSize` — text scale

| `zSize`   | Text              |
| --------- | ----------------- |
| `sm`      | `text-[0.625rem]` |
| `default` | `text-xs`         |
| `lg`      | `text-sm`         |

## `zPosition` — placement

Twelve placements: `top`, `top-left`, `top-right`, `bottom`, `bottom-left`, `bottom-right`, `left`, `left-top`,
`left-bottom`, `right`, `right-top`, `right-bottom`. Positioning auto-flips to a fallback placement when there
isn't room (via the shared `z-overlay` floating-ui service).

## Examples

```html
<!-- Icon button hint, always shown -->
<button z-button zType="ghost" zSize="sm" z-tooltip zContent="Refresh" [zAlwaysShow]="true">
  <z-icon zType="lucideRefreshCw" zSize="16" />
</button>

<!-- Truncated table cell, tooltip only appears when text overflows -->
<span class="block max-w-40 truncate" z-tooltip [zContent]="row.name">{{ row.name }}</span>

<!-- Click-triggered, colored -->
<span z-tooltip zContent="Copied!" zTrigger="click" zType="success" #tip="zTooltip" (click)="tip.show()">
  Click to copy
</span>

<!-- Rich content via TemplateRef -->
<span z-tooltip [zContent]="tipTpl">Hover for details</span>
<ng-template #tipTpl>
  <strong>Status:</strong>
  Active
</ng-template>
```

## Anti-patterns

```html
<!-- WRONG: native title attribute, no styling/positioning control -->
<button title="Delete">Delete</button>

<!-- WRONG: hand-rolled absolutely-positioned tooltip div -->
<div class="group relative">
  <button>Hover me</button>
  <div class="absolute hidden bg-black p-1 text-white group-hover:block">Tip</div>
</div>

<!-- WRONG: invented input, zTheme is not a real prop -->
<span z-tooltip zContent="Hi" zTheme="dark"></span>
```
