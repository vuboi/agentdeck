// Shared clock for things that count up on screen. `sec` moves every second (live timers), `min` every
// half minute (relative times like "4 minutes ago"), so a row that shows a date does not redraw each second.
export const ticker = $state({ sec: Date.now(), min: Date.now() });

if (typeof window !== 'undefined') {
  let beats = 0;
  setInterval(() => {
    ticker.sec = Date.now();
    if (++beats % 30 === 0) ticker.min = ticker.sec;
  }, 1000);
}
