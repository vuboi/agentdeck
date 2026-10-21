# Chat — `z-chat`

Selector: `z-chat` (`exportAs="zChat"`). A self-contained floating AI-assistant widget: a bottom-corner FAB that
opens a message panel with a virtualized message list, suggestion chips, file attachments, and a composer. It
renders its own FAB, panel, header, and footer — mount it once near the root of a page/layout, not per-section.

```html
<z-chat [(zOpen)]="chatOpen" zPosition="bottom-right" [zOffset]="{ right: 20, bottom: 90 }" />
```

## Inputs

| Input               | Type                              | Default                                     | Notes                                                                                                                          |
| ------------------- | --------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `class`             | `ClassValue`                      | `''`                                        | Extra classes merged onto the wrapper                                                                                          |
| `zOpen`             | `model<boolean>`                  | `false`                                     | Two-way bindable open/closed state — use `[(zOpen)]`                                                                           |
| `zMessages`         | `model<ZChatMessage[]>`           | `[]`                                        | Two-way bindable message list; virtualized, so mutate via `.set()`/`.update()` not by mutating in place                        |
| `zTitle`            | `string`                          | `'i18n_z_ui_chat_ai_title'`                 | Header title (i18n key, run through `TranslatePipe`)                                                                           |
| `zSubtitle`         | `string`                          | `'i18n_z_ui_chat_ai_subtitle'`              | Subtitle i18n key (declared, not rendered in the current header markup)                                                        |
| `zPlaceholder`      | `string`                          | `'i18n_z_ui_chat_ai_placeholder'`           | Composer textarea placeholder (i18n key)                                                                                       |
| `zPosition`         | `'bottom-right' \| 'bottom-left'` | `'bottom-right'`                            | Which corner the FAB/panel anchor to                                                                                           |
| `zOffset`           | `ZChatOffset`                     | `{}`                                        | Per-side pixel/CSS overrides (`top`/`right`/`bottom`/`left`, `number` or `string`); unset sides fall back to position defaults |
| `zSize`             | `'sm' \| 'default' \| 'lg'`       | `'default'`                                 | Panel height/width (see table below)                                                                                           |
| `zSuggestions`      | `ZChatSuggestion[]`               | `Z_CHAT_DEFAULT_SUGGESTIONS`                | Suggestion chips shown in the empty state (`{ label, prompt?, icon? }`)                                                        |
| `zShowSuggestions`  | `boolean`                         | `true`                                      | Whether the empty state renders suggestion chips                                                                               |
| `zShowPulse`        | `boolean`                         | `true`                                      | Idle attention-pulse ring on the closed FAB                                                                                    |
| `zDisabled`         | `boolean`                         | `false`                                     | Disables the composer textarea and attach button                                                                               |
| `zLoading`          | `boolean`                         | `false`                                     | Forces the typing indicator on (combines with internal auto-reply typing state)                                                |
| `zAllowAttachments` | `boolean`                         | `true`                                      | Show/enable the attach-file button and paste-to-attach                                                                         |
| `zAcceptedFiles`    | `string`                          | `'image/*,.pdf,.doc,.docx,.txt,.csv,.xlsx'` | `accept` attribute for the file input                                                                                          |
| `zAutoReply`        | `boolean`                         | `true`                                      | Simulate a mock assistant reply after each send (demo/offline mode)                                                            |
| `zAutoReplyDelay`   | `number`                          | `1200`                                      | Mock reply delay in ms (floored at 200ms internally)                                                                           |
| `zFabIcon`          | `ZIcon`                           | `'lucideMessageSquareDot'`                  | Icon on the closed FAB                                                                                                         |
| `zBotIcon`          | `ZIcon`                           | `'lucideBot'`                               | Icon in the panel header avatar                                                                                                |
| `zSendIcon`         | `ZIcon`                           | `'lucideArrowUp'`                           | Icon on the send button (replaced by a spinner while typing)                                                                   |

## Outputs

| Output      | Payload                                           | Fires when                                                                        |
| ----------- | ------------------------------------------------- | --------------------------------------------------------------------------------- |
| `zSend`     | `ZChatSendEvent` (`{ message, files, messages }`) | User sends a message (Enter or clicking send)                                     |
| `zResponse` | `ZChatMessage`                                    | The simulated auto-reply lands (only relevant while `zAutoReply` is true)         |
| `zToggle`   | `boolean`                                         | Panel opens or closes (FAB click, `open()`/`close()`, or Escape)                  |
| `zClear`    | `void`                                            | `clearMessages()` is called (the header's "new chat" button, or programmatically) |

## Public methods (via template ref / `zChat` exportAs)

| Method            | Notes                                                                |
| ----------------- | -------------------------------------------------------------------- |
| `open()`          | Opens the panel (no-op if already open); emits `zToggle`             |
| `close()`         | Closes the panel (no-op if already closed); emits `zToggle`          |
| `clearMessages()` | Empties `zMessages`, revokes attachment preview URLs, emits `zClear` |

## `zSize` — panel dimensions

| `zSize`   | Panel size            |
| --------- | --------------------- |
| `sm`      | `h-[28rem] w-[20rem]` |
| `default` | `h-[35rem] w-[25rem]` |
| `lg`      | `h-[39rem] w-[29rem]` |

There is no `zType`/style-variant axis — the FAB and panel styling is fixed (gradient FAB when closed, flat surface
when open); only `zPosition` and `zSize` are variant-driven.

## Examples

```html
<!-- Minimal floating widget, default position/size -->
<z-chat [(zOpen)]="chatOpen" />

<!-- Bottom-right, offset to sit above another fixed element -->
<z-chat [(zOpen)]="chatOpen" zPosition="bottom-right" [zOffset]="{ right: 20, bottom: 90 }" />

<!-- Custom suggestions, larger panel, real backend replies (no mock auto-reply) -->
<z-chat
  #chat="zChat"
  [(zOpen)]="chatOpen"
  [(zMessages)]="messages"
  zSize="lg"
  [zSuggestions]="mySuggestions"
  [zAutoReply]="false"
  [zLoading]="isWaitingForReply()"
  (zSend)="onSend($event)"
/>

<!-- Toggle it from an external button using the exportAs handle -->
<button z-button zType="outline" zSize="sm" (click)="chat.open()">Ask AI</button>

<!-- Disable attachments and drive everything as controlled state -->
<z-chat [(zOpen)]="chatOpen" [zAllowAttachments]="false" [zDisabled]="isSending()" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled floating chat bubble/panel, no virtualization, no a11y, no theming -->
<div class="fixed right-4 bottom-4 rounded-full bg-blue-600 p-4">
  <button (click)="open = !open">💬</button>
</div>

<!-- WRONG: invented input, there is no zType/zVariant on z-chat -->
<z-chat zType="minimal" [(zOpen)]="chatOpen" />

<!-- WRONG: mutating zMessages() in place instead of using the model -->
<!-- chat.zMessages().push(newMessage) -- won't trigger change detection or the virtualizer -->

<!-- RIGHT: update through the model signal -->
<!-- chat.zMessages.update(list => [...list, newMessage]) -->
```
