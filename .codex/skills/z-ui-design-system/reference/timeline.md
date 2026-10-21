# Timeline — `z-timeline`

Selector: `z-timeline`. Renders a vertical timeline/changelog from a `ZTimelineItem[]` array, with three layout
modes (changelog, zigzag, career/experience) and optional scroll-driven progress fill. Never hand-roll a
timeline with absolutely-positioned dots and borders.

```html
<z-timeline [zItems]="events" />
```

```ts
protected readonly events: ZTimelineItem[] = [
  { title: 'v2.0 released', description: 'Major redesign', time: 'Jan 2026' },
  { title: 'v1.5 released', description: 'Bug fixes', time: 'Nov 2025' },
];
```

## `ZTimelineItem` shape (each entry of `zItems`)

| Field             | Type                   | Notes                                                                                                                                        |
| ----------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`              | `string \| number`     | Optional track-by id (falls back to index)                                                                                                   |
| `title`           | `string`               | Required content — main heading for the entry                                                                                                |
| `description`     | `string`               | Optional body text                                                                                                                           |
| `time`            | `string`               | Optional date/time badge (`default`/`alternate` layouts)                                                                                     |
| `subtitle`        | `string`               | Optional secondary text (`default` layout) / fallback company label (`career` layout)                                                        |
| `company`         | `string`               | Optional, used as the header label in `career` layout                                                                                        |
| `period`          | `string`               | Optional date range shown with a calendar icon in `career` layout (falls back to `time`)                                                     |
| `technologies`    | `string[]`             | Optional chip list rendered under the description in `career` layout                                                                         |
| `icon`            | `ZIcon`                | Optional icon in the `career` layout's avatar circle (defaults to `lucideBuilding2`)                                                         |
| `active`          | `boolean`              | Declared on the type but not read by the component's own logic — don't rely on it to control the progress dot; use `zScrollProgress` instead |
| `contentTemplate` | `TemplateRef<unknown>` | Optional extra content rendered below the description in every layout                                                                        |

## Inputs

| Input             | Type                                   | Default     | Notes                                                                                                    |
| ----------------- | -------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------- |
| `class`           | `ClassValue`                           | `''`        | Extra classes on the host                                                                                |
| `zItems`          | `ZTimelineItem[]`                      | `[]`        | The timeline entries                                                                                     |
| `zSize`           | `'sm' \| 'default' \| 'lg'`            | `'default'` | Dot/badge/title/description text scale                                                                   |
| `zLayout`         | `'default' \| 'alternate' \| 'career'` | `'default'` | Layout mode (see below)                                                                                  |
| `zScrollProgress` | `boolean`                              | `false`     | Animate the connecting line filling in as the user scrolls past each item, and dim not-yet-reached items |

No outputs — this is a display-only component.

## `zLayout` — pick by content type

| `zLayout`   | Structure                                                                                                            | Use for                                  |
| ----------- | -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| `default`   | Single column: date/subtitle on the left (desktop) or above (mobile), dot + line in the center, content on the right | Changelogs, activity feeds, audit logs   |
| `alternate` | Zigzag — content cards alternate left/right of a centered line, each in a bordered card                              | Marketing/story timelines, process steps |
| `career`    | Single column, no side gutter: icon avatar + company header, title, period, description, tech chips                  | Resume/experience sections               |

## `zSize` — text and dot scale

| `zSize`   | Timeline text | Outer dot  | Inner dot  |
| --------- | ------------- | ---------- | ---------- |
| `sm`      | `text-sm`     | `size-4.5` | `size-2`   |
| `default` | `text-base`   | `size-6`   | `size-3`   |
| `lg`      | `text-lg`     | `size-7`   | `size-3.5` |

(`zSize` has no effect on the `career` layout, which uses its own fixed Tailwind classes in the template.)

## Examples

```html
<!-- Changelog / activity feed -->
<z-timeline [zItems]="releaseNotes" />

<!-- Zigzag process timeline -->
<z-timeline [zItems]="steps" zLayout="alternate" zSize="lg" />

<!-- Resume / experience section with scroll-driven progress -->
<z-timeline [zItems]="workHistory" zLayout="career" [zScrollProgress]="true" />

<!-- Extra content per item via contentTemplate -->
<ng-template #extra let-item>
  <button z-button zType="ghost" zSize="sm">View diff</button>
</ng-template>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled timeline with absolute-positioned dots -->
<div class="relative border-l pl-4">
  <div class="bg-primary absolute top-1 -left-1.5 size-3 rounded-full"></div>
  <h3>v2.0 released</h3>
</div>

<!-- WRONG: invented input, zOrientation is not a real prop -->
<z-timeline [zItems]="events" zOrientation="horizontal" />

<!-- WRONG: guessing a layout value that doesn't exist -->
<z-timeline [zItems]="events" zLayout="zigzag" />
```
