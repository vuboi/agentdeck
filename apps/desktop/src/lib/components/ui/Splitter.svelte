<script lang="ts">
  // A draggable divider. Reports the pointer movement along its axis; the owner clamps and stores it.
  let { axis, onDelta, label, dragging = $bindable(false) }: { axis: 'x' | 'y'; onDelta: (px: number) => void; label: string; dragging?: boolean } = $props();

  function startDrag(event: PointerEvent) {
    event.preventDefault();
    dragging = true;
    let last = axis === 'x' ? event.clientX : event.clientY;
    const move = (moved: PointerEvent) => {
      const now = axis === 'x' ? moved.clientX : moved.clientY;
      if (now !== last) onDelta(now - last);
      last = now;
    };
    const up = () => {
      dragging = false;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<!-- Same divider as the original panes: 1px line, primary glow on hover and while dragging (agent-divider in app.css). -->
<div
  role="separator"
  aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
  aria-label={label}
  data-dragging={dragging}
  class={axis === 'x'
    ? "agent-divider agent-divider-v w-px shrink-0 cursor-col-resize bg-border [&>*]:hidden after:absolute after:inset-y-0 after:-left-1 after:w-[9px] after:content-['']"
    : "agent-divider agent-divider-h h-px w-full shrink-0 cursor-row-resize bg-border [&>*]:hidden after:absolute after:inset-x-0 after:-top-1 after:h-[9px] after:content-['']"}
  onpointerdown={startDrag}
></div>
