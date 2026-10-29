<!--
  Phyllotaxis indicator adapted from daniasyrofi/loading/specimens (https://github.com/daniasyrofi/loading).
  MIT License
  Copyright (c) 2026 Dani Asyrofi (https://daniasyrofi.com)

  Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
  documentation files (the "Software"), to deal in the Software without restriction, including without limitation
  the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and
  to permit persons to whom the Software is furnished to do so, subject to the following conditions:

  The above copyright notice and this permission notice shall be included in all copies or substantial portions of
  the Software.

  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
  THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
  CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
  IN THE SOFTWARE.
-->
<script lang="ts">
  /** One seed in the golden-angle spiral. */
  type Seed = { index: number; x: number; y: number; size: number; delay: number };

  const seeds: Seed[] = Array.from({ length: 24 }, (_, index) => {
    const angle = (index * 137.50776 * Math.PI) / 180;
    const radius = 2.25 * Math.sqrt(index);
    return {
      index,
      x: 21 + radius * Math.cos(angle),
      y: 13 + radius * Math.sin(angle),
      size: 1.05 + radius * 0.055,
      delay: radius / 11,
    };
  });

  let { className = '' }: { className?: string } = $props();
</script>

<svg class="phyllotaxis {className}" viewBox="8 0 26 26" fill="none" aria-hidden="true">
  <g class="phyllotaxis__field">
    {#each seeds as seed (seed.index)}
      <circle class="phyllotaxis__seed" style:animation-delay="-{1800 * seed.delay}ms" cx={seed.x} cy={seed.y} r={seed.size} />
    {/each}
  </g>
</svg>

<style>
  .phyllotaxis {
    display: block;
    width: 0.875rem;
    height: 0.875rem;
    flex: none;
    color: currentColor;
  }

  .phyllotaxis__field {
    transform-box: view-box;
    transform-origin: 21px 13px;
    animation: phyllo-turn 3600ms linear infinite;
  }

  .phyllotaxis__seed {
    fill: currentColor;
    transform-box: fill-box;
    transform-origin: center;
    animation: phyllo-pulse 1800ms ease-in-out infinite;
  }

  @keyframes phyllo-turn {
    to {
      rotate: 360deg;
    }
  }

  @keyframes phyllo-pulse {
    0%,
    100% {
      opacity: 0.92;
      scale: 1.2;
    }
    46% {
      opacity: 0.18;
      scale: 0.58;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .phyllotaxis__field,
    .phyllotaxis__seed {
      animation: none;
    }
  }
</style>
