<script lang="ts">
  // A small heading for a block inside a panel: the label, then a hairline running to the right edge.
  let { label }: { label: string } = $props();
</script>

<div class="mb-2 flex items-center gap-2" role="presentation">
  <span class="shrink-0 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">{label}</span>
  <span class="h-px flex-1 bg-border" aria-hidden="true"></span>
</div>
