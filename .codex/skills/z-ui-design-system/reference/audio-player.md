# Audio Player — `z-audio-player`

Selector: `z-audio-player`. A compact audio preview player with a generated waveform, play/pause control,
download action, optional title/tags, and click-or-drag seeking. Use it for audio previews in file galleries,
attachments, and media cards instead of building custom audio controls around a raw `<audio>` element.

```html
<z-audio-player
  zSrc="/assets/preview.mp3"
  zTitle="Preview"
  [zTags]="['Audio', 'Preview']"
  (zPlay)="onPlayStateChange($event)"
/>
```

## Inputs

| Input       | Type                | Default | Notes                                                              |
| ----------- | ------------------- | ------- | ------------------------------------------------------------------ |
| `zSrc`      | `string` (required) | —       | Audio URL. The browser must be able to load the resource.          |
| `zTitle`    | `string`            | `''`    | Optional title shown below the waveform.                           |
| `zTags`     | `readonly string[]` | `[]`    | Optional tags shown beside the title.                              |
| `zDownload` | `boolean`           | `true`  | Shows the download action and uses the title as the download name. |

## Outputs

| Output    | Payload   | Fires when                                           |
| --------- | --------- | ---------------------------------------------------- |
| `zLoaded` | `void`    | Audio metadata has loaded and duration is available. |
| `zError`  | `Error`   | The audio resource fails to load.                    |
| `zPlay`   | `boolean` | Playback starts (`true`) or pauses/ends (`false`).   |

## Seeking and keyboard interaction

- Click the waveform to seek to that position.
- Press and drag across the waveform to scrub continuously; pointer capture keeps the drag active.
- The waveform exposes `role="slider"` and updates `aria-valuenow` in seconds.
- `ArrowLeft`/`ArrowRight` seek by 5 seconds; hold `Shift` to seek by 10 seconds.
- `Home` seeks to the beginning and `End` seeks to the end.

The waveform is the seek surface, not the generated bar elements. Keep the player mounted while seeking so its
metadata and duration state remain available.

## Examples

```html
<!-- Minimal audio preview -->
<z-audio-player zSrc="/assets/voice-note.mp3" />

<!-- File preview with metadata and no download button -->
<z-audio-player
  zSrc="/files/interview.mp3"
  zTitle="Interview recording"
  [zTags]="['Interview', 'MP3']"
  [zDownload]="false"
/>

<!-- Track playback and load failures -->
<z-audio-player
  zSrc="audioUrl"
  (zLoaded)="onAudioLoaded()"
  (zError)="onAudioError($event)"
  (zPlay)="onAudioPlay($event)"
/>
```

## Anti-patterns

```html
<!-- WRONG: bypasses the Z-UI waveform, seek behavior, and design tokens -->
<audio src="/assets/preview.mp3" controls></audio>

<!-- WRONG: these inputs/outputs do not exist -->
<z-audio-player [zAutoplay]="true" (zTimeUpdate)="onTime($event)" />

<!-- WRONG: zTitle is a string input, not a signal/model for two-way binding -->
<z-audio-player [(zTitle)]="title" />
```
