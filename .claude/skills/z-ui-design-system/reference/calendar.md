# Calendar — `z-calendar`

Selector: `z-calendar`. A single date/range/time picker built as an input trigger + popover calendar. Implements
`ControlValueAccessor`, so it works with `[(ngModel)]` or `formControlName` like a normal form field. Never
build a date picker by hand with a raw `<input type="date">` or a manual popover + grid.

```html
<z-calendar zLabel="Date of birth" zPlaceholder="Pick a date..." [(ngModel)]="birthDate" />

<z-calendar zMode="range" zLabel="Date range" [(ngModel)]="range" />

<!-- Reactive form with a backend-friendly date string -->
<z-calendar
  formControlName="dueDate"
  zMode="single"
  zValueType="string"
  zFormat="yyyy-MM-dd"
  zPlaceholder="yyyy-MM-dd"
  zLabel="Due date"
/>
```

## Inputs

| Input               | Type                                                              | Default        | Notes                                                                                                                         |
| ------------------- | ----------------------------------------------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `class`             | `ClassValue`                                                      | `''`           | Extra classes merged with trigger variant classes                                                                             |
| `zMode`             | `'single' \| 'range' \| 'time' \| 'month' \| 'year' \| 'quarter'` | `'single'`     | Picker mode                                                                                                                   |
| `zSize`             | `'sm' \| 'default' \| 'lg'`                                       | `'default'`    | Trigger height                                                                                                                |
| `zLabel`            | `string`                                                          | `''`           | Label text above the trigger                                                                                                  |
| `zLabelClass`       | `string`                                                          | `''`           | Extra classes on the label                                                                                                    |
| `zPlaceholder`      | `string \| null`                                                  | `null`         | Input placeholder                                                                                                             |
| `zRequired`         | `boolean`                                                         | `false`        | Shows a red `*` next to the label                                                                                             |
| `zDisabled`         | `boolean`                                                         | `false`        | Disabled state                                                                                                                |
| `zReadonly`         | `boolean`                                                         | `false`        | Read-only — opens for viewing but not editable/clearable                                                                      |
| `zLoading`          | `boolean`                                                         | `false`        | Shows a spinner icon in the trigger                                                                                           |
| `zLoadingOutline`   | `boolean`                                                         | `false`        | Adds a focus-ring-style outline while `zLoading`                                                                              |
| `zShowTime`         | `boolean`                                                         | `false`        | Adds a time-of-day picker below the calendar (single/range modes)                                                             |
| `zTimeFormat`       | `'12h' \| '24h'`                                                  | `'24h'`        | Time picker format                                                                                                            |
| `zShowHour`         | `boolean`                                                         | `true`         | Show hour column in the time picker                                                                                           |
| `zShowMinute`       | `boolean`                                                         | `true`         | Show minute column                                                                                                            |
| `zShowSecond`       | `boolean`                                                         | `true`         | Show second column                                                                                                            |
| `zQuickSelect`      | `boolean`                                                         | `false`        | Shows preset shortcuts sidebar (e.g. "Last 7 days") — range mode                                                              |
| `zAllowEdit`        | `boolean`                                                         | `true`         | Allow typing directly into the input; `false` makes it click-to-open only                                                     |
| `zShortTime`        | `boolean`                                                         | `false`        | Collapse a matched quick-select preset into a single compact input label                                                      |
| `zAllowClear`       | `boolean`                                                         | `true`         | Show the clear ("x") button when a value is set                                                                               |
| `zFormat`           | `string`                                                          | `'DD/MM/YYYY'` | Display/parse format tokens (`YYYY`, `MM`, `DD`, `HH`, `hh`, `mm`, `ss`, `A`/`a`); `yyyy` and `dd` aliases are also supported |
| `zMinDate`          | `Date \| null`                                                    | `null`         | Minimum selectable date                                                                                                       |
| `zMaxDate`          | `Date \| null`                                                    | `null`         | Maximum selectable date                                                                                                       |
| `zValueType`        | `'date' \| 'iso' \| 'string'`                                     | `'date'`       | Emitted/bound value shape                                                                                                     |
| `zValidators`       | `ZCalendarValidator[]`                                            | `[]`           | Custom validators (`{ error, message, validate }`) mapped to form-control errors                                              |
| `zShowOk`           | `boolean`                                                         | `false`        | Force-show an OK/Apply button in the footer                                                                                   |
| `zOkText`           | `string \| null`                                                  | `null`         | Override OK button text                                                                                                       |
| `zShowCancel`       | `boolean`                                                         | `false`        | Force-show a Cancel button in the footer                                                                                      |
| `zCancelText`       | `string \| null`                                                  | `null`         | Override Cancel button text                                                                                                   |
| `zDisabledDate`     | `((date: Date) => boolean) \| null`                               | `null`         | Per-date disable predicate                                                                                                    |
| `zScrollClose`      | `boolean`                                                         | `false`        | Close the popover when the page scrolls                                                                                       |
| `zDefaultTime`      | `ZCalendarDefaultTime \| null`                                    | `null`         | Default time (`'current' \| 'start' \| 'end' \| {hour,minute,second}`) applied when opening without a value                   |
| `zRangeDefaultTime` | `ZCalendarRangeDefaultTime \| null`                               | `null`         | Per-side (`start`/`end`) default time for range mode                                                                          |

## Selection hover motion

The calendar popover uses `zHoverGlide` for selectable day, month, year, quarter, and desktop range quick-select
items. The two range calendars each own their selection highlight, and the mobile quick-select control delegates to
`z-select`, which already provides the same behavior. Active, range, today, and disabled state classes remain on the
items; ordinary pointer-hover backgrounds are provided by the shared highlight so a selection does not render two
hover surfaces.

## Outputs

| Output     | Payload                                                    | Notes                                                                                                                                                                                                                |
| ---------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `zControl` | `ZCalendarControl`                                         | Emitted once on init; exposes `valid()`, `reset()`, `focus()`, `open()`, `close()`, `clear()`, `setErrorMessage()`, and signals (`hasError`, `isOpen`, `value`, `errorMessage`) for imperative control from a parent |
| `zChange`  | `ZCalendarValue` (`Date \| {start,end} \| string \| null`) | Fires on value change                                                                                                                                                                                                |
| `zOnBlur`  | `FocusEvent`                                               | Trigger blur                                                                                                                                                                                                         |
| `zOnFocus` | `FocusEvent`                                               | Trigger focus (popover open)                                                                                                                                                                                         |
| `zEvent`   | `ZEvent`                                                   | Generic unified focus/blur event wrapper                                                                                                                                                                             |

## `zSize` — trigger height

| `zSize`   | Height        | Use for                         |
| --------- | ------------- | ------------------------------- |
| `sm`      | `h-8` (32px)  | Compact/dense forms             |
| `default` | `h-9` (36px)  | Standard forms — default choice |
| `lg`      | `h-10` (40px) | Prominent forms, dialogs        |

## Public methods (via template ref or `exportAs="zCalendar"`)

`open()`, `close()`, `toggle()`, `clear()`, `focus()`, `blur()`, `reset()`, `valid(): boolean`,
`setErrorMessage(message: string | null)`. `validate(): boolean` also exists but is deprecated in favor of
`valid()`.

## Examples

```html
<!-- Single date, bound to ngModel -->
<z-calendar zLabel="Start date" zPlaceholder="DD/MM/YYYY" [(ngModel)]="startDate" zRequired="true" />

<!-- Range with quick-select presets -->
<z-calendar zMode="range" zLabel="Report period" [zQuickSelect]="true" [(ngModel)]="reportRange" />

<!-- Date + time, 12h format, OK/Cancel footer -->
<z-calendar zLabel="Appointment" [zShowTime]="true" zTimeFormat="12h" [zShowOk]="true" [(ngModel)]="appointment" />

<!-- Formatted string value instead of a Date object -->
<z-calendar
  zLabel="Return ISO"
  zFormat="yyyy-MM-dd"
  zPlaceholder="yyyy-MM-dd"
  zValueType="string"
  [(ngModel)]="isoDate"
/>

<!-- Disable weekends -->
<z-calendar zLabel="Pickup date" [zDisabledDate]="isWeekend" [(ngModel)]="pickupDate" />

<!-- Imperative control -->
<z-calendar zLabel="Due date" [(ngModel)]="dueDate" (zControl)="calendarControl = $event" />
<button z-button (click)="calendarControl.clear()">Clear</button>
```

## Anti-patterns

```html
<!-- WRONG: native date input, no formatting/locale/range/time support, no validation UI -->
<input type="date" [(ngModel)]="date" />

<!-- WRONG: invented prop, there is no zColor -->
<z-calendar zColor="blue" [(ngModel)]="date" />

<!-- WRONG: guessing a size that doesn't exist -->
<z-calendar zSize="xl" [(ngModel)]="date" />

<!-- WRONG: zMode="range" but binding a plain Date — range mode expects {start, end} -->
<z-calendar zMode="range" [(ngModel)]="singleDate" />
```
