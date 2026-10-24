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
    // `data-tip-clip`: only when the label inside is really cut short (a tab title), never for a label that fits.
    if (element.hasAttribute('data-tip-clip')) {
      const label = element.querySelector<HTMLElement>('.truncate') ?? (element as HTMLElement);
      if (label.scrollWidth <= label.clientWidth) return;
    }
    const box = element.getBoundingClientRect();
    const below = box.top < 44;
    tip = { text, x: box.left + box.width / 2, y: below ? box.bottom + 6 : box.top - 6, below };
  }

  /** Centres the card on the anchor, then pulls it back inside the window (it may be wider than the room at one side). */
  function place(card: HTMLElement) {
    if (!tip) return;
    const margin = 8;
    const width = card.offsetWidth;
    const height = card.offsetHeight;
    const left = Math.min(Math.max(tip.x - width / 2, margin), Math.max(margin, window.innerWidth - width - margin));
    let top = tip.below ? tip.y : tip.y - height;
    top = Math.min(Math.max(top, margin), Math.max(margin, window.innerHeight - height - margin));
    card.style.left = `${left}px`;
    card.style.top = `${top}px`;
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
  <!-- In the browser's top layer, like the popovers: no z-index can lift a tooltip above those. Shown last, it is on top. -->
  <div
    popover="manual"
    {@attach (element) => {
      element.showPopover?.();
      place(element);
    }}
    class="pointer-events-none fixed inset-auto m-0 w-max max-w-[min(18rem,calc(100vw-16px))] overflow-visible rounded-lg border border-border bg-popover px-2.5 py-1.5 text-xs font-medium break-words whitespace-normal text-popover-foreground shadow-lg"
    role="tooltip"
  >
    {tip.text}
  </div>
{/if}
