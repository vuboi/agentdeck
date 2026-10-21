# Kanban — `z-kanban`

Selector: `z-kanban`, `exportAs="zKanban"`. A full drag-and-drop task board (columns, cards, search, a task
editor modal with description/checklist/attachments/comments) built on Angular CDK drag-drop. It owns its own
UI — you supply `zColumns` data and listen to a handful of outputs; never hand-roll column/card markup or
your own `cdkDropList`/`cdkDrag` wiring around it.

```html
<z-kanban [(zColumns)]="columns" (zTaskDrop)="onTaskDrop($event)" (zTaskSave)="onTaskSave($event)" />
```

## Inputs

| Input              | Type                         | Default                             | Notes                                                                                                     |
| ------------------ | ---------------------------- | ----------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `class`            | `ClassValue`                 | `''`                                | Extra classes on the host                                                                                 |
| `zColumns`         | `model<ZKanbanColumn[]>`     | `[]`                                | Two-way bindable board data — use `[(zColumns)]`; the component mutates it internally on drag/create/save |
| `zTitle`           | `string` (i18n key)          | `'i18n_z_ui_kanban_board_title'`    | Toolbar heading                                                                                           |
| `zSubtitle`        | `string` (i18n key)          | `'i18n_z_ui_kanban_board_subtitle'` | Toolbar subheading                                                                                        |
| `zSearch`          | `model<string>`              | `''`                                | Two-way bindable search query; while non-empty, drag is disabled and columns are filtered                 |
| `zShowToolbar`     | `boolean`                    | `true`                              | Show/hide the title+search+add-task toolbar                                                               |
| `zAllowCreateTask` | `boolean`                    | `true`                              | Show the "add task" affordances (toolbar button + per-column `+`)                                         |
| `zDisabled`        | `boolean`                    | `false`                             | Disables drag-and-drop and task creation                                                                  |
| `zDensity`         | `'compact' \| 'comfortable'` | `'comfortable'`                     | Column/card padding density                                                                               |

## Outputs

| Output        | Payload                                                                                          | Fires when                                                            |
| ------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------- |
| `zTaskDrop`   | `ZKanbanTaskDropEvent` (`taskId`, `fromColumnId`, `toColumnId`, `previousIndex`, `currentIndex`) | A task is dropped (reordered within a column or moved across columns) |
| `zTaskSave`   | `ZKanbanTaskSaveEvent` (`task`, `columnId`, `previousTask`)                                      | The task editor modal is saved, or a comment is added                 |
| `zTaskClick`  | `ZKanbanTask`                                                                                    | A task card is clicked and the editor opens                           |
| `zTaskCreate` | `ZKanbanTaskCreateEvent` (`task`, `columnId`)                                                    | A new task is created (via toolbar or per-column `+`)                 |

## Public methods (template ref API, `exportAs="zKanban"`)

| Method                             | Purpose                                                           |
| ---------------------------------- | ----------------------------------------------------------------- |
| `createTaskInFirstColumn()`        | Programmatically create a task in the first column                |
| `onBoardSearchChange(value)`       | Set the search query (used internally by the toolbar's `z-input`) |
| `createTask(columnId)`             | Create a task in a specific column                                |
| `openTaskEditor(columnId, taskId)` | Open the editor modal for a given task                            |
| `closeEditor()`                    | Close the editor modal                                            |
| `saveTaskChanges()`                | Commit the editor's draft state to the task                       |

## Data shapes

`ZKanbanColumn = { id, title, tasks: ZKanbanTask[] }`. `ZKanbanTask` requires `id`, `title`, `status`,
`priority` (`'risk' | 'high' | 'medium' | 'low'`), `assignees: string[]`, plus optional `code`, `description`
(HTML from `z-editor`), `startDate`/`endDate`/`dueDate` (`YYYY-MM-DD` strings), `commentsCount`,
`attachmentsCount`, `attachments: ZUploadFile[]`, `checklist: ZKanbanChecklistItem[]`, `activity:
ZKanbanActivityItem[]`, `completed`. Column `id` values that match `'done'` mark tasks dropped into them as
`completed: true` automatically. Full shapes in
[z-kanban.types.ts](../../../../libs/core-ui/components/z-kanban/z-kanban.types.ts).

## No `zSize`/`zType` — density is the only visual variant

There's no `zSize` or `zType` on `z-kanban`; the only CVA-driven visual switch is `zDensity`
(`compact`/`comfortable`), which adjusts column and card padding
(`p-2.5`/`space-y-2 p-2.5` vs `p-3`/`space-y-2.5 p-3'` — see
[z-kanban.variants.ts](../../../../libs/core-ui/components/z-kanban/z-kanban.variants.ts)). Priority-based
card border/accent colors and the priority badge colors are fixed (not user-configurable) and keyed off
`ZKanbanPriority`, not a variant prop you pass in.

## Examples

```html
<!-- Basic board -->
<z-kanban [(zColumns)]="columns" />

<!-- Read-only board, no toolbar, compact cards -->
<z-kanban
  [(zColumns)]="columns"
  [zShowToolbar]="false"
  [zAllowCreateTask]="false"
  [zDisabled]="true"
  zDensity="compact"
/>

<!-- Persist moves to the backend -->
<z-kanban
  #board="zKanban"
  [(zColumns)]="columns"
  (zTaskDrop)="api.moveTask($event)"
  (zTaskSave)="api.updateTask($event.task)"
/>
<button z-button (click)="board.createTaskInFirstColumn()">New task</button>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled columns with raw cdkDropList/cdkDrag instead of z-kanban -->
<div cdkDropList *ngFor="let col of columns">
  <div cdkDrag *ngFor="let task of col.tasks">{{ task.title }}</div>
</div>

<!-- WRONG: invented prop, there is no zColor -->
<z-kanban [(zColumns)]="columns" zColor="blue" />

<!-- WRONG: one-way binding on zColumns loses drag/create/save mutations -->
<z-kanban [zColumns]="columns" />
```
