<script lang="ts">
  // The AgentDeck mark: two cards of a deck fanned on the primary tile, a spark on the front one.
  let { class: className = '' }: { class?: string } = $props();
  const id = $props.id();
</script>

<svg viewBox="0 0 24 24" class="shrink-0 {className}" aria-hidden="true">
  <defs>
    <linearGradient id="{id}-gloss" x1="0" y1="0" x2="0.6" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.32" />
      <stop offset="1" stop-color="#fff" stop-opacity="0" />
    </linearGradient>
    <linearGradient id="{id}-shade" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0.55" stop-color="#000" stop-opacity="0" />
      <stop offset="1" stop-color="#000" stop-opacity="0.22" />
    </linearGradient>
  </defs>
  <rect width="24" height="24" rx="6.5" class="fill-primary" />
  <rect width="24" height="24" rx="6.5" fill="url(#{id}-shade)" />
  <rect width="24" height="24" rx="6.5" fill="url(#{id}-gloss)" />
  <rect x="5" y="6.5" width="9" height="11.5" rx="2.2" fill="#fff" fill-opacity="0.45" transform="rotate(-14 9.5 12.25)" />
  <rect x="9.5" y="6" width="9" height="12" rx="2.2" fill="#fff" />
  <path d="M14 8.9Q14.45 11.55 17 12Q14.45 12.45 14 15.1Q13.55 12.45 11 12Q13.55 11.55 14 8.9Z" class="fill-primary" />
</svg>
