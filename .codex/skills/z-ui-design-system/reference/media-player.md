# Media Player — `z-media-player`

Selector: `z-media-player`. A Vidstack Web Components player using one of Vidstack's prebuilt skins
(`zSkin`: `default` or `plyr`). Video, audio, HLS, DASH, YouTube, and Vimeo sources are supported.

```html
<z-media-player [zSources]="videoSources" [zConfig]="{ poster: '/assets/intro-poster.jpg', title: 'Intro' }" />
```

## Inputs

| Input                  | Type                              | Default     | Notes                                                                                                                                                                                                                                                                      |
| ---------------------- | --------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`                | `ClassValue`                      | `''`        | Extra classes merged with the root variant classes.                                                                                                                                                                                                                        |
| `zSources`             | `ZMediaPlayerSource[]`            | required    | `{ label, kind, src, type?, default? }[]`; `kind` explicitly selects the provider. `type` pins an exact MIME type for `video`/`audio` kinds; leave it unset to let Vidstack infer it (`blob:` URLs -> `video/object`, otherwise a HEAD request) — ignored for other kinds. |
| `zConfig`              | `Partial<ZMediaPlayerConfig>`     | `{}`        | Mirrors Vidstack's own `<media-player>` props (see below), plus `subtitles`/`thumbnailsVtt`/`chaptersVtt`.                                                                                                                                                                 |
| `zSkin`                | `'default' \| 'plyr'`             | `'default'` | Which prebuilt Vidstack skin renders the whole player UI.                                                                                                                                                                                                                  |
| `zDefaultLayoutConfig` | `ZMediaPlayerDefaultLayoutConfig` | `{}`        | Vidstack's real `DefaultLayoutProps`, applied only when `zSkin` is `'default'`.                                                                                                                                                                                            |
| `zPlyrLayoutConfig`    | `ZMediaPlayerPlyrLayoutConfig`    | `{}`        | Vidstack's real `PlyrLayoutProps`, applied only when `zSkin` is `'plyr'`.                                                                                                                                                                                                  |

`ZMediaPlayerConfig` is `Partial<Omit<MediaPlayerProps, 'src' | 'autoplay' | 'crossorigin' | 'playsinline'>>`
re-exported from the real `vidstack` package types (plus the three extra fields above). It exposes Vidstack's
real property names such as `autoPlay`, `playsInline`, `crossOrigin`, `controls`, `playbackRate`, `preload`,
`streamType`, and more. Do not hand-declare or rename these fields; import the type from the component entrypoint.

`zDefaultLayoutConfig` and `zPlyrLayoutConfig` likewise use Vidstack's real layout types. Examples include
`colorScheme`, `download`, `seekStep`, `playbackRates`, `noModal`, `sliderChaptersMinWidth` (Default), or
`controls`, `markers`, `speed`, and `invertTime` (Plyr). When the app language is `vi`, the component supplies
bundled Vietnamese translations; per-instance `translations` values override them.

## Outputs

| Output         | Payload                   | Fires when                                                                                                                                   |
| -------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `zStateChange` | `ZMediaPlayerStateChange` | Normalized Vidstack state changes (`loaded`, `play`, `pause`, time, volume, rate, fullscreen, PiP, source, subtitles, waiting, or can-play). |
| `zError`       | `ZMediaPlayerError`       | A provider, thumbnail-VTT, or chapter-VTT operation fails.                                                                                   |

The `zStateChange` payload contains `event` and a normalized `state` with `paused`, `playing`, `ended`, `waiting`,
`currentTime`, `duration`, `volume`, `muted`, `playbackRate`, `bufferedRatio`, `fullscreen`, `theaterMode`,
`pictureInPicture`, `activeSourceLabel`, and `activeCaptionsLabel`.

## Providers

Use explicit source kinds instead of URL sniffing:

```ts
const sources: ZMediaPlayerSource[] = [
  { label: 'Lesson', kind: 'video', src: '/videos/lesson.mp4', default: true },
  { label: 'Live HLS', kind: 'hls', src: 'https://example.com/live.m3u8' },
  { label: 'DASH', kind: 'dash', src: 'https://example.com/manifest.mpd' },
  { label: 'YouTube', kind: 'youtube', src: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ' },
  { label: 'Vimeo', kind: 'vimeo', src: 'https://vimeo.com/76979871' },
];
```

The first source marked `default` becomes active; otherwise the first source is used. HLS and DASH load their
optional streaming libraries through the provider-change hook. Both skins expose adaptive quality choices when
the provider supplies them.

## Skins

Vidstack ships two prebuilt skins; switch the whole UI with `zSkin`, no other input changes:

```html
<z-media-player [zSources]="videoSources" zSkin="plyr" [zPlyrLayoutConfig]="{ seekTime: 5 }" />
```

There is no headless/custom-control-bar mode — always use one of the two prebuilt skins.

## Examples

```html
<!-- Minimal player -->
<z-media-player [zSources]="[{ label: 'Default', kind: 'video', src: '/assets/intro.mp4', default: true }]" />

<!-- Subtitles and timeline storyboard -->
<z-media-player
  [zSources]="videoSources"
  [zConfig]="{
  poster: '/assets/intro-poster.jpg',
  subtitles: [{ label: 'English', src: '/subs/intro-en.vtt', srclang: 'en', default: true }],
  thumbnailsVtt: '/videos/intro-storyboard.vtt'
}"
/>

<!-- Plyr skin with a custom seek step -->
<z-media-player [zSources]="videoSources" zSkin="plyr" [zPlyrLayoutConfig]="{ seekTime: 5 }" />

<!-- Default skin with layout-specific tuning -->
<z-media-player [zSources]="videoSources" [zDefaultLayoutConfig]="{ seekStep: 5, noAudioGain: true }" />
```

## Anti-patterns

```html
<!-- WRONG: bypasses provider selection and the Vidstack skin entirely. -->
<video src="video.mp4" controls></video>

<!-- WRONG: provider kind is inferred from a URL instead of declared on the source. -->
<z-media-player [zSources]="[{ label: 'Video', src: videoUrl }]" />

<!-- WRONG: `autoplay` is Vidstack's deprecated alias; the real prop name is `autoPlay`. -->
<z-media-player [zSources]="videoSources" [zConfig]="{ autoplay: true }" />

<!-- WRONG: zPlyrLayoutConfig only applies when zSkin is 'plyr' — this has no effect on the default skin. -->
<z-media-player [zSources]="videoSources" [zPlyrLayoutConfig]="{ seekTime: 5 }" />
```
