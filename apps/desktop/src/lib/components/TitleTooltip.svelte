<script lang="ts">
  import { onMount } from 'svelte';

  // Buttons carry plain `title` text; the system draws those as a pale native box. This takes them over: on hover
  // the text moves to `data-tip` (so the native one never shows) and appears as the app's own small card.
  let tip = $state<{ text: string; x: number; y: number; below: boolean } | null>(null);
  let timer = 0;
  let current: Element | null = null;

  function show(element: Element) {
    const text = element.getAttribute('data-tip');
    if (!text) return;
    const box = element.getBoundingClientRect();
    const below = box.top < 44;
    tip = { text, x: Math.min(Math.max(box.left + box.width / 2, 8), window.innerWidth - 8), y: below ? box.bottom + 6 : box.top - 6, below };
  }

  function hide() {
    clearTimeout(timer);
    tip = null;
    current = null;
  }

  onMount(() => {
    const over = (event: PointerEvent) => {
      const element = (event.target as Element | null)?.closest?.('[title], [data-tip]') ?? null;
      if (element === current) return;
      hide();
      if (!element || element.closest('svg')) return;
      const title = element.getAttribute('title');
      if (title) {
        element.setAttribute('data-tip', title);
        element.removeAttribute('title');
      }
      current = element;
      timer = window.setTimeout(() => current && show(current), 450);
    };
    document.addEventListener('pointerover', over, true);
    document.addEventListener('pointerdown', hide, true);
    document.addEventListener('scroll', hide, true);
    window.addEventListener('blur', hide);
    return () => {
      document.removeEventListener('pointerover', over, true);
      document.removeEventListener('pointerdown', hide, true);
      document.removeEventListener('scroll', hide, true);
      window.removeEventListener('blur', hide);
    };
  });
</script>

{#if tip}
  <div
    class="pointer-events-none fixed z-[100] max-w-72 rounded-lg border border-border bg-popover px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-popover-foreground shadow-lg"
    style:left="{tip.x}px"
    style:top="{tip.y}px"
    style:transform={tip.below ? 'translateX(-50%)' : 'translate(-50%, -100%)'}
    role="tooltip"
  >
    {tip.text}
  </div>
{/if}
