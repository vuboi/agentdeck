# Menu — `z-menu-v2` (primary) / `z-menu` (legacy)

A self-contained application sidebar/navigation shell: logo header, collapsible desktop rail, mobile drawer with
backdrop, nested menu tree driven by a data array (not content projection), and a user footer popover. Both
`ZMenuV2Component` (selector `z-menu-v2`) and `ZMenuComponent` (selector `z-menu`) are exported from
`@shival99/z-ui/components/z-menu`, but **`z-menu-v2` is the one to use for new work** — it has a cleaner
expand/collapse model (`zExpandMode`), animated submenu open/close, and menu group labels. `z-menu` (v1) is kept
for existing consumers and adds one feature v2 doesn't have: per-parent custom template content
(`menuTemplate`/`zActionsTemplate`) rendered in a dedicated desktop panel / mobile drawer. Reach for v1 only if
you specifically need that custom-template-per-menu-item behavior; otherwise use v2.

Never hand-roll a sidebar nav with raw `<aside>` + `<ul>` — you lose responsive collapse, mobile drawer,
active-path syncing with the Angular `Router`, and the built-in user menu popover.

```html
<z-menu-v2
  [zMenus]="menuItems"
  [zUser]="{ name: 'Jane Doe', email: 'jane@acme.com', avatar: 'assets/avatar.png' }"
  [zUserActions]="userActions"
  zLogo="assets/logo.svg"
  zTitle="Acme Admin"
  (zOnSelect)="onMenuSelect($event)"
  (zOnMenuAction)="onUserAction($event)"
/>
```

## Inputs (`z-menu-v2`)

| Input                 | Type                        | Default                      | Notes                                                                                                   |
| --------------------- | --------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------- |
| `zLogo`               | `string`                    | `'assets/images/avatar.svg'` | Desktop header logo image src                                                                           |
| `zLogoMobile`         | `string`                    | `''`                         | Mobile bar/drawer logo; falls back to `zLogo` if empty                                                  |
| `zMenus`              | `ZMenuV2Item[]`             | `[]`                         | Menu tree data (up to 3 levels: parent → child → sub-child)                                             |
| `zUser`               | `ZMenuUser \| null`         | `null`                       | `{ name, email, avatar? }` shown in footer + user popover                                               |
| `zUserActions`        | `ZMenuUserAction[]`         | `[]`                         | Actions listed in the user popover (see type below)                                                     |
| `zCurrentPath`        | `string`                    | `''`                         | Explicit active path; falls back to the Angular `Router`'s current URL when empty                       |
| `zKey`                | `string`                    | `'z-menu-v2'`                | Cache key for persisting `sidebarCollapsed` state across sessions                                       |
| `class` (as `zClass`) | `string`                    | `''`                         | Extra classes merged onto the shell                                                                     |
| `zTitle`              | `string`                    | `''`                         | Header title text (translated); defaults to a built-in i18n key if empty                                |
| `zCollapsedWidth`     | `number`                    | `60`                         | Rail width in px when collapsed                                                                         |
| `zExpandedWidth`      | `number`                    | `248`                        | Rail width in px when expanded                                                                          |
| `zExpandMode`         | `'accordion' \| 'multiple'` | `'multiple'`                 | `'accordion'` closes other top-level parents when one expands; `'multiple'` allows several open at once |
| `zShowMobileBar`      | `boolean`                   | `false`                      | Shows the top mobile bar (logo + hamburger) below `md` breakpoint                                       |

## Outputs (`z-menu-v2`)

| Output          | Payload           | Notes                                                      |
| --------------- | ----------------- | ---------------------------------------------------------- |
| `zOnSelect`     | `ZMenuItem`       | Fires when a leaf menu item is selected                    |
| `zOnMenuAction` | `ZMenuUserAction` | Fires when a (non-disabled) user popover action is clicked |
| `zControl`      | `ZMenuV2Control`  | Emitted once (async) with an imperative API — see below    |

## Imperative control (`zControl`)

Bind `(zControl)="menuControl = $event"` to get a `ZMenuV2Control` object exposing:
`openMobileMenu()`, `closeMobileMenu()`, `toggleMobileMenu()`, `setSelectedMenuItem(id)`, `clearActiveItem()`,
`toggleSidebar()`, and read-only signals `sidebarCollapsed`, `mobileMenuOpen`, `selectedMenuItem`,
`selectedParent`, `menuProcessed`. This is the supported way to drive the menu from outside (e.g. a header
hamburger button toggling `toggleMobileMenu()`) instead of reaching into component internals.

## Key data types

```ts
interface ZMenuV2Item {
  id: string;
  name: string;
  icon?: ZIcon;
  iconSvg?: string;
  path: string;
  children?: ZMenuV2Item[]; // up to 3 levels deep
  active?: boolean;
  expanded?: boolean;
  permission?: string[];
  subPaths?: string[]; // additional URLs that should also mark this item active
  projects?: string[];
  groupLabel?: string; // renders a section divider + label above this top-level item
}

interface ZMenuUser {
  name: string;
  email: string;
  avatar?: string;
}

interface ZMenuUserAction {
  id: string;
  label: string;
  icon?: ZIcon;
  disabled?: boolean;
  hidden?: boolean;
  divide?: 'before' | 'after'; // renders a separator line before/after this action
  class?: string;
}
```

No CVA variants file exists for `z-menu`/`z-menu-v2` (no `zType`/`zSize` visual variants) — sizing is controlled
purely via `zCollapsedWidth`/`zExpandedWidth`, and there is no size/variant table to document.

## Hover motion and radius

`z-menu` (v1) and `z-menu-v2` use `zHoverGlide` for the rail, desktop child levels, mobile tree levels, and built-in
user-action lists. Each list owns one shared highlight; nested hosts own their own items, and menu items must keep
`data-glide-item` without adding a second pointer-hover background on the same surface. Active and hover item corners
use `rounded-sm`, so they follow the theme's `--radius-sm` value (default `0.125rem`). Custom `menuTemplate`
content is projected content and is not automatically included in the built-in glide lists.

## `z-menu` (v1) — additional inputs beyond the v2 set

`z-menu` covers the same `zLogo`, `zLogoMobile`, `zMenus` (as `ZMenuItem[]`), `zUser`, `zUserActions`,
`zCurrentPath`, `zKey`, `zOnSelect`, `zOnMenuAction`, and `zControl` (typed `ZMenuControl`, which additionally
exposes `openMobileTemplateDrawer(parentId)` / `closeMobileTemplateDrawer()` / `mobileTemplateDrawerOpen` signal).
It does not have `zExpandMode`, `zCollapsedWidth`/`zExpandedWidth`, `zShowMobileBar`, or group labels. Its
distinguishing feature is per-item custom content:

| Input              | Type                                         | Default | Notes                                                                  |
| ------------------ | -------------------------------------------- | ------- | ---------------------------------------------------------------------- |
| `zActionsTemplate` | `TemplateRef<{ close: () => void }> \| null` | `null`  | Custom template rendered in the user-footer area, given a `close()` fn |

Additionally, `ZMenuItem` (v1's item type) has `menuTemplate?: TemplateRef<ZMenuItemTemplateContext>` and
`menuTemplateWidth?: number` — set these on a parent item to render arbitrary projected content (via
`NgTemplateOutlet`, context `{ $implicit: item, close: () => void }`) instead of its children list, in both the
desktop side-panel and the mobile drawer opened by `openMobileTemplateDrawer(parentId)`.

## Examples

```html
<!-- v2: standard app shell usage -->
<z-menu-v2
  [zMenus]="menuItems"
  [zUser]="currentUser()"
  [zUserActions]="[
    { id: 'profile', label: 'Profile', icon: 'lucideUser' },
    { id: 'logout', label: 'Log out', icon: 'lucideLogOut', divide: 'before' }
  ]"
  zTitle="Acme Admin"
  [zExpandMode]="'accordion'"
  [zShowMobileBar]="true"
  (zOnSelect)="router.navigateByUrl($event.path)"
  (zOnMenuAction)="handleUserAction($event)"
/>

<!-- v2: grabbing imperative control to toggle from a custom header -->
<z-menu-v2 [zMenus]="menuItems" (zControl)="menuControl.set($event)" />
<button z-button zType="ghost" (click)="menuControl()?.toggleSidebar()">
  <z-icon zType="lucidePanelLeft" />
</button>

<!-- v1: parent item with fully custom panel content -->
<!-- component.ts -->
<!-- menus = [{ id: 'reports', name: 'Reports', path: '/reports', menuTemplate: reportsTpl, menuTemplateWidth: 320 }] -->
<z-menu [zMenus]="menus" (zOnSelect)="onSelect($event)"></z-menu>
<ng-template #reportsTpl let-item let-close="close">
  <app-reports-panel (done)="close()" />
</ng-template>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled sidebar, no responsive collapse/mobile drawer, no router-active sync -->
<aside class="h-full w-64 border-r">
  <ul>
    <li *ngFor="let m of menus"><a [routerLink]="m.path">{{ m.name }}</a></li>
  </ul>
</aside>

<!-- WRONG: invented input, zVariant is not a real prop -->
<z-menu-v2 zVariant="compact" [zMenus]="menuItems"></z-menu-v2>

<!-- WRONG: projecting menu markup as content — z-menu-v2 takes menu structure as `zMenus` data, not projection -->
<z-menu-v2>
  <a *ngFor="let m of menuItems" [routerLink]="m.path">{{ m.name }}</a>
</z-menu-v2>

<!-- WRONG: using v1's menuTemplate/menuTemplateWidth on v2 items — z-menu-v2's ZMenuV2Item has no menuTemplate -->
<z-menu-v2 [zMenus]="[{ id: 'x', name: 'X', path: '/x', menuTemplate: tpl }]"></z-menu-v2>
```
