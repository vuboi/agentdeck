# Tabs — `z-tabs`

Selector: `z-tabs`. Renders a tab list from a `ZTab[]` data array plus content panels, marked with the
`z-tab-panel` structural-style directive on an `<ng-template>`. Never hand-roll a tab bar with buttons and
`*ngIf`s.

```html
<z-tabs [zTabs]="tabs" [(activeTab)]="activeTab">
  <ng-template z-tab-panel="overview">Overview content...</ng-template>
  <ng-template z-tab-panel="settings">Settings content...</ng-template>
</z-tabs>
```

```ts
protected readonly tabs: ZTab[] = [
  { label: 'Overview', value: 'overview' },
  { label: 'Settings', value: 'settings', icon: 'lucideSettings' },
];
protected readonly activeTab = signal('overview');
```

## `ZTab` shape (each entry of `zTabs`)

| Field          | Type                   | Notes                                                                                                                                                                                                                                                     |
| -------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `label`        | `string`               | Passed through `TranslatePipe`, so an i18n key works directly                                                                                                                                                                                             |
| `value`        | `string`               | Unique id, bound to `activeTab` / `z-tab-panel`                                                                                                                                                                                                           |
| `icon`         | `ZIcon`                | Optional leading icon, named from a bundled `@ng-icons` set                                                                                                                                                                                               |
| `iconTemplate` | `TemplateRef<unknown>` | Renders the leading mark from a template instead of `icon`, for marks `z-icon` cannot draw — a multi-colour brand SVG, a flag, an avatar. Receives the `ZTab` as its implicit context, so one template serves a whole strip. Takes precedence over `icon` |
| `disabled`     | `boolean`              | Optional                                                                                                                                                                                                                                                  |
| `badge`        | `string \| number`     | Optional trailing count/label badge                                                                                                                                                                                                                       |
| `template`     | `TemplateRef<unknown>` | Optional — set internally when matched to a `z-tab-panel`; don't set by hand                                                                                                                                                                              |

## Inputs

| Input                 | Type                                               | Default        | Notes                                                                                                           |
| --------------------- | -------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------- |
| `class`               | `ClassValue`                                       | `''`           | Extra classes on the tab list                                                                                   |
| `zTabs`               | `ZTab[]`                                           | required       | The tab definitions                                                                                             |
| `zType`               | see table below                                    | `'line'`       | Visual style                                                                                                    |
| `zSize`               | `'xs' \| 'sm' \| 'default' \| 'lg'`                | `'default'`    | Tab item height/padding                                                                                         |
| `zOrientation`        | `'horizontal' \| 'vertical'`                       | `'horizontal'` | Layout direction                                                                                                |
| `zContentFit`         | `'none' \| 'screen'`                               | `'none'`       | Sets `data-content-fit` for layout hooks (e.g. full-screen panel content)                                       |
| `zContentAnimation`   | `'none' \| 'fade' \| 'slide' \| 'blur' \| 'scale'` | `'fade'`       | How the entering panel animates in; `slide`/`blur` read the travel direction and follow the strip's axis        |
| `zLazy`               | `boolean`                                          | `false`        | Keep only the active panel in the DOM; the swap waits out the tab animation so heavy content never drops frames |
| `zTabsOnly`           | `boolean`                                          | `false`        | Render only the tab list, no panel container (useful when panels live elsewhere)                                |
| `zShowNavWrapper`     | `boolean`                                          | `false`        | Add a bordered/elevated surface around supported glide-type navigation                                          |
| `zNavFullWidth`       | `boolean`                                          | `false`        | Make the horizontal navigation area fill the available width when `zShowNavWrapper` is enabled                  |
| `zClassContent`       | `ClassValue`                                       | `''`           | Extra classes merged into the tab navigation surface/container                                                  |
| `zShowContentWrapper` | `boolean`                                          | `true`         | Wrap panels in the framed content box (only visible for `folder*`/`sharp-line` types, horizontal orientation)   |
| `zReloadable`         | `boolean`                                          | `true`         | Whether clicking the already-active tab re-triggers `zTabChange`                                                |
| `zEmitOnActiveClick`  | `boolean`                                          | `true`         | Whether clicking the already-active tab emits `zTabChange` at all                                               |
| `zCache`              | `boolean`                                          | `true`         | Persist/restore `activeTab` per page path via `ZCacheService`                                                   |
| `zCacheKey`           | `string`                                           | `''`           | Explicit cache key, overriding the auto-generated `z-tabs:<path>:<instance>` key                                |
| `activeTab`           | `model<string>`                                    | `''`           | Two-way bindable active tab value — use `[(activeTab)]`                                                         |

## Outputs

| Output       | Payload                          | Fires when                                                      |
| ------------ | -------------------------------- | --------------------------------------------------------------- |
| `zTabChange` | `string` (the new tab's `value`) | A tab is selected (respects `zReloadable`/`zEmitOnActiveClick`) |

## Directive: `z-tab-panel` / `zTabPanel`

Apply to an `<ng-template>` as a static attribute value matching a tab's `value`:

```html
<ng-template z-tab-panel="overview">...</ng-template>
```

`exportAs="zTabPanel"`. Public input: `zTabPanel: string` (required) — the matching tab value.

## `zType` — visual style (grounded in `z-tabs.variants.ts`)

| `zType`                                   | Character                                                                                                                                                                                                                                                                                                                                               |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `line`                                    | Minimal underline that slides in on the active tab (default)                                                                                                                                                                                                                                                                                            |
| `soft`                                    | Rounded pill background on the active tab, no border                                                                                                                                                                                                                                                                                                    |
| `segment`                                 | Segmented-control look inside a muted track — active tab's pill glides between positions (see below)                                                                                                                                                                                                                                                    |
| `segment-solid`                           | Segmented control, active tab filled with `bg-primary` — same gliding pill                                                                                                                                                                                                                                                                              |
| `pill-solid`                              | Fully-rounded (`rounded-full`) segmented control, gliding solid `bg-background` pill                                                                                                                                                                                                                                                                    |
| `pill-blur`                               | Like `pill-solid` but the gliding pill is a frosted "liquid glass" (`backdrop-filter: blur() saturate()`, glossy highlight, translucent edge ring) — use over busy/colorful backgrounds                                                                                                                                                                 |
| `pill-float`                              | Track-less bare strip: gliding inverted neutral pill (`bg-foreground` / `text-background`), masked edges, and circular scroll arrows floating over those edges on overflow (the overflow dropdown is hidden)                                                                                                                                            |
| `pill-primary`                            | Track-less bare strip with a gliding solid `bg-primary` pill (`text-primary-foreground` label)                                                                                                                                                                                                                                                          |
| `assistant`                               | Muted pill track where only the active tab keeps its label — the others collapse to their icon and the pill widens as the label expands (needs `icon` set on each tab). The collapse is horizontal-only; a vertical rail has room, so it keeps every label                                                                                              |
| `underline-hover`                         | Hairline strip with a gliding active underline plus a second muted highlight that follows the pointer and keyboard focus                                                                                                                                                                                                                                |
| `bar`                                     | Bare rows with a `primary/10` active box (`rounded-sm`, lifted by `shadow-sm`) and a 4px `primary` rule inset to 25% of the leading edge, rounded on its trailing side — bottom when horizontal, inline-start when vertical. Laid out vertically the rail also carries the divider (`border-e border-border` + `pe-4`) that separates it from the panel |
| `folder` / `folder-soft` / `folder-solid` | File-folder tabs with a connecting bottom border to the content frame                                                                                                                                                                                                                                                                                   |
| `underline` / `underlined`                | Border-bottom style active indicator                                                                                                                                                                                                                                                                                                                    |
| `sharp-line`                              | Joined boxed tabs with a thick bottom border, rounded only at the strip's two ends                                                                                                                                                                                                                                                                      |
| `liquid`                                  | Muted pill track; the `bg-primary` pill stretches across both tabs while moving (leading edge first, tail catching up) and settles on the target                                                                                                                                                                                                        |
| `glow`                                    | Hairline strip; a glowing `primary` rule under the active tab with soft light spilling onto it (side rule when vertical)                                                                                                                                                                                                                                |
| `gradient`                                | A slowly rotating conic-gradient ring around the active tab over a faint `primary` tint (static under reduced motion)                                                                                                                                                                                                                                   |
| `bracket`                                 | Viewfinder corner brackets framing the active tab over a faint `primary` tint                                                                                                                                                                                                                                                                           |
| `vertical-sharp`                          | Sharp-style border indicator meant for `zOrientation="vertical"`                                                                                                                                                                                                                                                                                        |

### The gliding indicator

The glide types share one absolutely-positioned indicator element that measures the active tab button
(`offsetLeft`/`offsetTop`/`offsetWidth`/`offsetHeight`) and animates `transform` + `width`/`height` onto
it with a spring-like easing plus a brief squash on change — instead of each tab toggling its own
background instantly. Any tab whose `icon` is set also gets a small scale-bounce on click, independent of
`zType`.

Two groups, listed in `z-tabs.define.ts`:

- `Z_TABS_GLIDE_TYPES` — `segment`, `segment-solid`, `pill-solid`, `pill-blur`, `pill-float`. These glide
  only for `zOrientation="horizontal"`; laid out vertically they fall back to a static per-item active
  background.
- `Z_TABS_OMNI_GLIDE_TYPES` — `pill-primary`, `underline-hover`, `bar`. These glide in either
  orientation; `bar` and `underline-hover` just move their rule to the side edge when vertical.

`assistant` splits by orientation, and the split follows from the motion rather than from taste:

- **Horizontal** it opts out of the shared indicator (`Z_TABS_COLLAPSING_LABEL_TYPES`) and paints its
  active surface on the tab itself, exactly as the reference does. That is not a shortcut — it is the
  only way the motion works. The active tab is resizing while the indicator would be travelling, so a
  shared box can only chase a width that has not settled; making the pill _be_ the tab's background
  means it grows with the label for free.
- **Vertical** it glides normally (`Z_TABS_VERTICAL_GLIDE_TYPES`). A rail keeps every label, so no row
  changes size, the constraint above disappears, and the indicator can just slide between rows like
  `pill-primary` and `bar` do. The rail's track is also lightened (`bg-muted/40`), because the full
  `bg-muted` of the compact horizontal strip reads as a heavy slab once it is several rows tall. `width: auto` cannot be interpolated, so the component measures the
  label's content width (`scrollWidth`, which reports it even while the label is clamped to zero) and
  writes it inline for the stylesheet to transition from `0`. Spacing rides on the label's
  `margin-inline-start` rather than the row's `gap`, because `gap` would keep reserving space beside an
  icon whose label is zero-width. One 300ms `cubic-bezier(0.4, 0, 0.2, 1)` covers the tab's background,
  its width, the entering label and the leaving one, so the whole strip reflows as a single motion.

`underline-hover` additionally glides a second, muted highlight behind the hovered tab
(`Z_TABS_HOVER_GLIDE_TYPES`). It tracks keyboard focus too, so tabbing through the strip reads the same as
hovering, and it is driven by plain CSS transitions rather than the Web Animations API — a dropped frame on
a hover affordance costs nothing.

All four also carry a hover surface (`muted/50`) and a `focus-visible` ring, which the older types
suppress. The values are taken from the shadcnspace reference: `shadow-xs` on the `assistant` pill and its
300ms `cubic-bezier(0.4, 0, 0.2, 1)` curve, `bar`'s `primary/10` active box under `shadow-sm`, and the
4px rule inset to 25% and rounded on its trailing side. Two details there are easy to get wrong, and both were
measured rather than read off the class list:

1. **The shadow is real even though the source disables it.** `tabs-06.tsx` writes
   `shadow-none data-active:shadow-none`, but shadcn's own
   `group-data-[variant=default]/tabs-list:data-active:shadow-sm` wins that cascade, so the page paints
   `shadow-sm` on the active row. Without it the tint just stops flat against the page.
2. **The pale rim around the active box is not a border.** The row carries a 1px _transparent_ border
   (`border border-dashed` whose colour shadcn overrides to transparent in both states), the `shadow-sm`
   is cast from that border box, and the `primary/10` fill is an `inset-0` child — which resolves
   against the **padding box**, putting the fill exactly 1px inside the shadow's origin (measured:
   173.6x40 inside a 175.2x41.6 row). The row's own background is transparent, so that 1px gap shows
   the page through, and _that_ is the rim the design reads as a border.

Z-UI reproduces both from the single indicator element: `inset 0 0 0 1px var(--background)` pushes the
tint 1px in while the two outer shadows stay cast from the full box. Copy the rim that way rather than
adding a real border — a border would sit outside the shadow instead of inside it.

Dark mode inverts what the rim is made of. `--background` is _darker_ than the surface these tabs sit
on (`--card`), so reusing it there draws a black line around the box, and the drop shadow the rim was
separating is nearly invisible on a dark ground anyway. The dark override swaps it for
`color-mix(in oklab, white 12%, transparent)` — a faint highlight that still defines the edge, matching
how `pill-blur` is edged in dark. Note where `bar`'s visible edge comes from: the
hairline belongs to the **rail**, not to the rows — every row is bare in both states, and it is the
rail's inline-end border that draws the line between the tabs and the panel.

Nothing to configure — it's automatic for these types. Don't try to recreate this by hand with a
`::after` line or manual transitions; it doesn't exist for the other `zType` values (`line` and
`underline` use their own, simpler sliding-underline `::after`, unrelated to this indicator).

When `zShowNavWrapper` is enabled, only glide types receive the bordered/elevated navigation surface.
`zNavFullWidth` controls whether that surface spans the available horizontal width; `zClassContent`
adds classes to the navigation container itself.

## `zContentAnimation` — how the entering panel appears

One animation is played on the panel that just became active, in both the eager and the `zLazy` path.
`slide` and `blur` are direction-aware: the panel enters from the side the click travelled towards
(compared by tab index), along the strip's own axis — X when the tabs run across, Y when they run down.
Everything except `blur` animates only `transform` and `opacity`, so the animation ticks on the compositor
and a heavy panel mounting in the same frame cannot drop its frames; `blur` repaints the panel each frame,
which is why its distance and duration stay small. Every option is skipped under
`prefers-reduced-motion`, and `'none'` disables it outright.

```html
<z-tabs [zTabs]="tabs" zType="pill-primary" zContentAnimation="slide" [(activeTab)]="activeTab">
  <ng-template z-tab-panel="overview">Overview content...</ng-template>
</z-tabs>
```

## Corner radius

Every type that is not part of the pill family uses `rounded-sm` — rows, tracks, the elevated nav
wrapper and the gliding indicator alike, so a strip cannot end up with a row radius that disagrees with
its track. `rounded-full` stays on the pill family (`pill-solid`, `pill-blur`, `pill-float`,
`pill-primary`, `assistant`, `liquid`) and `rounded-none` stays on the sharp family (`sharp-line`,
`underlined`, `vertical-sharp`, `glow`, `bracket`) — those radii are the identity of those types.
Tailwind's `rounded-sm` compiles to a literal `0.3125rem` here, which is why the indicator's SCSS
hardcodes the same number rather than reading a `--radius-*` token.

A vertical rail is only a scroll container once it holds more tabs than fit (`z-tabs-scroll-wrapper-limited`,
past 8 tabs). Below that it stays `overflow: visible`, because a box-shadow is not part of scrollable
overflow — clipping the rail unconditionally cut the active indicator's shadow off at all four edges.

## `zSize` — tab item padding

| `zSize`   | Classes                                   |
| --------- | ----------------------------------------- |
| `xs`      | `min-h-8 px-2.5 py-1 text-xs`             |
| `sm`      | `min-h-9 px-3 py-1.5 text-sm`             |
| `default` | `min-h-10 px-4 py-2 text-sm`              |
| `lg`      | `min-h-[2.625rem] px-4.5 py-2.25 text-sm` |

Horizontal tab lists auto-scroll and show a dropdown-overflow button (via `z-popover`) when tabs overflow the
available width — no extra configuration needed.

## Examples

```html
<!-- Segmented control style, lazy-loaded panels -->
<z-tabs [zTabs]="tabs" [(activeTab)]="activeTab" zType="segment" [zLazy]="true">
  <ng-template z-tab-panel="overview">...</ng-template>
  <ng-template z-tab-panel="billing">...</ng-template>
</z-tabs>

<!-- Vertical settings-style nav -->
<z-tabs [zTabs]="settingsTabs" [(activeTab)]="activeSection" zOrientation="vertical" zType="soft">
  <ng-template z-tab-panel="profile">...</ng-template>
  <ng-template z-tab-panel="security">...</ng-template>
</z-tabs>

<!-- Tab bar only, panels rendered elsewhere -->
<z-tabs [zTabs]="tabs" [(activeTab)]="activeTab" [zTabsOnly]="true" (zTabChange)="onRouteChange($event)" />

<!-- Floating over a photo/video background: pill-blur reads as glass instead of a flat box -->
<div class="bg-[url(/hero.jpg)] bg-cover bg-center p-6">
  <z-tabs [zTabs]="iconTabs" [(activeTab)]="activeTab" zType="pill-blur" [zTabsOnly]="true" />
</div>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled tab buttons + *ngIf panels -->
<div class="flex gap-2">
  <button (click)="tab = 'a'" [class.active]="tab === 'a'">A</button>
  <button (click)="tab = 'b'" [class.active]="tab === 'b'">B</button>
</div>
<div *ngIf="tab === 'a'">...</div>

<!-- WRONG: invented input, zVariant is not a real prop -->
<z-tabs [zTabs]="tabs" zVariant="pills" />

<!-- WRONG: passing a plain string array instead of ZTab[] -->
<z-tabs [zTabs]="['overview', 'settings']" />
```

### Heavy panel content

`z-tabs` receives each panel as an `ng-template`, so it cannot split that content itself — the instantiation cost is the consumer's to manage.

- `zLazy` keeps only the active panel mounted. The incoming view is built and bound off-document (no style/layout/paint until it is inserted) and committed on the frame after the click, so the tab strip paints first; the glide itself runs on the compositor and cannot be stalled by the mount. The outgoing view is detached immediately but destroyed on an idle callback, keeping teardown out of the clicked frame. Panels are still destroyed, so content that needs a fresh start gets one.
- For a panel that is genuinely expensive, wrap its body in `@defer` with a `@placeholder`. The panel then mounts a cheap skeleton and the real subtree is instantiated in a separate task. Measured on a 400-row panel: main-thread blocking dropped from ~67ms to ~39ms per switch.
- Inactive panels are `content-visibility: hidden`, so an `on viewport` trigger stays dormant until its tab is actually shown.
- `@defer` reduces the block but does not remove it: one `@defer` still instantiates its whole block at once. To remove it, do not create the nodes — virtualise the list (`@tanstack/angular-virtual`, CDK `cdk-virtual-scroll-viewport`) or page it.

```html
<ng-template zTabPanel="reports">
  @defer (on viewport) {
  <heavy-report />
  } @placeholder {
  <z-skeleton zType="bar" [zRows]="6" />
  }
</ng-template>
```
