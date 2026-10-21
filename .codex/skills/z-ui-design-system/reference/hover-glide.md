# Hover Glide — `[zHoverGlide]`

`zHoverGlide` is a low-level directive for a shared hover highlight over list items. It is used internally by
select, autocomplete, command, dropdown-menu, `z-menu`, and `z-calendar` selection surfaces. Use it when a custom
list needs one animated highlight that follows pointer movement; do not add it to a list that already supplies its
own hover treatment.

## Inputs

| Input                 | Type      | Default                            | Notes                                                          |
| --------------------- | --------- | ---------------------------------- | -------------------------------------------------------------- |
| `zHoverGlideSelector` | `string`  | `'[data-glide-item]'`              | CSS selector, relative to the host, for hoverable items        |
| `zHoverGlideClass`    | `string`  | `'bg-muted rounded-(--radius-sm)'` | Classes applied to the absolutely positioned highlight element |
| `zHoverGlideDisabled` | `boolean` | `false`                            | Disables pointer tracking and highlight animation              |

The host gets `relative`. Add `data-glide-item` to each hoverable item, or pass a selector that matches the items:

```html
<div zHoverGlide class="flex flex-col gap-1 p-1">
  <button z-button zType="ghost" data-glide-item class="relative z-[1] justify-start">Overview</button>
  <button z-button zType="ghost" data-glide-item class="relative z-[1] justify-start">Activity</button>
</div>
```

The directive delegates one `pointerover` listener from the container, so it works with virtualized lists. Touch
input is ignored, disabled items are skipped, and reduced-motion preferences disable the transform animation.
Keep item content above the highlight with a stacking context such as `relative z-[1]` when the highlight could
otherwise cover it.

There is exactly one highlight surface per host. It is marked with `data-z-hover-glide-highlight`; stale sibling
surfaces are removed before reuse. Moving, leaving, or destroying the host cancels tracked and untracked Web
Animations and resets the last box, so a cancelled glide cannot leave a ghost at the previous item.

`z-menu` and `z-menu-v2` create one host for each rail, menu level, mobile tree level, and user-action list. Their
interactive items use the theme's `--radius` token for active/hover surfaces. Nested hosts own their own items, so a
parent host ignores pointer events from a child host. `z-calendar` applies the directive to day, month, year,
quarter, and range quick-select lists (including both sides of a range calendar). Calendar variant classes keep
selection/range backgrounds, while ordinary pointer hover backgrounds are supplied by the shared glide.

## Anti-patterns

```html
<!-- WRONG: the selector does not match any item, so no highlight can move. -->
<div zHoverGlide [zHoverGlideSelector]="'.option'">
  <button data-glide-item>Option</button>
</div>

<!-- WRONG: each item owns a separate animated background when one shared glide is intended. -->
<div class="group">
  <button class="group-hover:bg-accent">One</button>
  <button class="group-hover:bg-accent">Two</button>
</div>
```
