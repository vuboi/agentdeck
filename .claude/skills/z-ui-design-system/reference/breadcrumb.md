# Breadcrumb — `z-breadcrumb`

Selector: `z-breadcrumb`. Renders a `<nav><ol>` trail from a plain data array — handles the active/last-item
styling, separators, `RouterLink` navigation, disabled items, and i18n translation of labels. Never hand-roll
a breadcrumb with manual `<a>` + `/` separators.

```html
<z-breadcrumb
  [zItems]="[
    { label: 'Home', path: '/' },
    { label: 'Settings', path: '/settings' },
    { label: 'Profile' },
  ]"
/>
```

## Inputs

| Input            | Type                        | Default                | Notes                                                                                                    |
| ---------------- | --------------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------- |
| `class`          | `ClassValue`                | `''`                   | Extra classes merged with variant classes on the `<ol>`                                                  |
| `zItems`         | `ZBreadcrumbItem[]`         | `[]`                   | `{ label, path?, icon?, disabled? }[]` — trail items, last item renders as the current page (no link)    |
| `zSize`          | `'sm' \| 'default' \| 'lg'` | `'default'`            | Text size                                                                                                |
| `zSeparator`     | `ZIcon \| string`           | `'lucideChevronRight'` | Separator between items — a `ZIcon` name (renders `z-icon`) or any other string rendered as literal text |
| `zSeparatorSize` | `ZIconVariants['zSize']`    | `'14'`                 | Icon pixel size for separators and the home icon                                                         |
| `zHomeIcon`      | `ZIcon \| undefined`        | `undefined`            | Leading "home" icon shown before the trail; only renders when `zShowHome` is also `true`                 |
| `zShowHome`      | `boolean`                   | `false`                | Show the home icon entry                                                                                 |

## Outputs

| Output       | Payload           | Fires when                                                  |
| ------------ | ----------------- | ----------------------------------------------------------- |
| `zItemClick` | `ZBreadcrumbItem` | A non-last, non-disabled item (or the home icon) is clicked |

Note: `path` on an item drives an `[routerLink]` for non-last, non-disabled items — routing happens
automatically, `zItemClick` is for side effects (analytics, etc.), not for performing navigation yourself.

## `zSize` — text scale

| `zSize`   | Text        |
| --------- | ----------- |
| `sm`      | `text-xs`   |
| `default` | `text-sm`   |
| `lg`      | `text-base` |

## Examples

```html
<!-- Basic trail -->
<z-breadcrumb [zItems]="[{ label: 'Docs', path: '/docs' }, { label: 'Installation' }]" />

<!-- With home icon -->
<z-breadcrumb
  [zShowHome]="true"
  zHomeIcon="lucideHouse"
  [zItems]="[{ label: 'Projects', path: '/projects' }, { label: 'Z-UI' }]"
/>

<!-- Icons per item, custom text separator -->
<z-breadcrumb
  zSeparator="/"
  [zItems]="[
    { label: 'Team', path: '/team', icon: 'lucideUsers' },
    { label: 'Members' },
  ]"
/>

<!-- Disabled intermediate item, track clicks -->
<z-breadcrumb
  [zItems]="[
    { label: 'Archive', path: '/archive', disabled: true },
    { label: 'Q1 2024' },
  ]"
  (zItemClick)="onCrumb($event)"
/>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled breadcrumb, no aria-current, no translation, manual separators -->
<nav>
  <a href="/">Home</a>
  /
  <a href="/settings">Settings</a>
  /
  <span>Profile</span>
</nav>

<!-- WRONG: invented prop, zLinks is not real -->
<z-breadcrumb [zLinks]="items" />

<!-- WRONG: guessing a size that doesn't exist -->
<z-breadcrumb zSize="xs" [zItems]="items" />
```
