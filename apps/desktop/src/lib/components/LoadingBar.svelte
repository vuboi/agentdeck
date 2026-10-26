<script lang="ts">
  // The chat's opening bar (app.css `.chat-loading`), for any pane that waits on its content: a hairline along the top
  // instead of a spinner in the middle, so the pane keeps its shape and the content fades in under it.
  // No out transition: the bar is part of the placeholder that the content replaces, and fading it out kept that whole
  // placeholder on the page under the content for a moment, which then jumped up when it went.
  let { label = 'Loading' }: { label?: string } = $props();
</script>

<div class="chat-loading pointer-events-none absolute inset-x-0 top-0 z-20 h-[2px] overflow-hidden" role="status" aria-label={label}>
  <div class="chat-loading-bar"></div>
  <div class="chat-loading-bar chat-loading-bar-trail"></div>
</div>
