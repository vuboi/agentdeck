// When a usage limit ends, read from the words of the error a CLI gave: "resets 12pm (Asia/Bangkok)" (Claude),
// "try again at Oct 28th, 2026 3:31 AM" or "try again in 4 hours 59 minutes" (Codex). Used to continue the work on its
// own once the limit has reset (sessions.svelte.ts). Free of stores and Tauri so plain node can test it.

const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'];
const CUE = /(?:resets?|try again|retry|available again|come back)/i;

const UNIT_MS: [RegExp, number][] = [
  [/^d/i, 86_400_000],
  [/^h/i, 3_600_000],
  [/^m/i, 60_000],
  [/^s/i, 1000],
];

/** "4 hours 59 minutes", "2h 30m", "45 min", "3 days" → milliseconds, or null. */
function parseDuration(text: string): number | null {
  let total = 0;
  let found = false;
  for (const match of text.matchAll(/(\d+(?:\.\d+)?)\s*(d(?:ays?)?|h(?:ours?|rs?)?|m(?:in(?:ute)?s?)?|s(?:ec(?:ond)?s?)?)\b/gi)) {
    const unit = UNIT_MS.find(([pattern]) => pattern.test(match[2]));
    if (!unit) continue;
    total += Number(match[1]) * unit[1];
    found = true;
  }
  return found ? total : null;
}

/** The offset of `zone` from UTC at an instant, in milliseconds. */
function offsetOf(instant: number, zone: string): number {
  const parts = new Intl.DateTimeFormat('en-US', { timeZone: zone, hourCycle: 'h23', year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric', second: 'numeric' }).formatToParts(new Date(instant));
  const at = (type: string) => Number(parts.find((part) => part.type === type)?.value ?? 0);
  return Date.UTC(at('year'), at('month') - 1, at('day'), at('hour'), at('minute'), at('second')) - Math.floor(instant / 1000) * 1000 - (instant % 1000 < 0 ? 1000 : 0);
}

/** The instant that is `year-month-day hour:minute` on the clocks of `zone`. */
function inZone(year: number, month: number, day: number, hour: number, minute: number, zone: string): number {
  const wall = Date.UTC(year, month, day, hour, minute);
  let instant = wall - offsetOf(wall, zone);
  // A second pass: the offset at the answer can differ from the one at the guess (daylight saving).
  instant = wall - offsetOf(instant, zone);
  return instant;
}

function zoneOk(zone: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** The date parts `instant` has on the clocks of `zone`. */
function partsIn(instant: number, zone: string) {
  const parts = new Intl.DateTimeFormat('en-US', { timeZone: zone, year: 'numeric', month: 'numeric', day: 'numeric' }).formatToParts(new Date(instant));
  const at = (type: string) => Number(parts.find((part) => part.type === type)?.value ?? 0);
  return { year: at('year'), month: at('month') - 1, day: at('day') };
}

/**
 * When the limit in `message` ends, as a time in ms, or null when the message does not say. `now` and `localZone` are
 * parameters so a test can fix them. A time with no date is the next time the clock shows it.
 */
export function parseResetAt(message: string, now = Date.now(), localZone = Intl.DateTimeFormat().resolvedOptions().timeZone): number | null {
  const cue = CUE.exec(message);
  if (!cue) return null;
  const tail = message.slice(cue.index + cue[0].length, cue.index + cue[0].length + 120);

  // "in 4 hours 59 minutes"
  const relative = /^\s*(?:again\s*)?in\s+([^.\n(]+)/i.exec(tail);
  if (relative) {
    const span = parseDuration(relative[1]);
    if (span !== null) return now + span;
  }

  // "at Oct 28th, 2026 3:31 AM (Asia/Bangkok)", "12pm (Asia/Bangkok)", "4:30am", "Oct 28, 12pm"
  const absolute = /^\s*(?:again\s*)?(?:at|on)?\s*(?:(?:[A-Za-z]+,?\s*)?(?:(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(\d{1,2})(?:st|nd|rd|th)?(?:,?\s*(\d{4}))?|(\d{4})-(\d{2})-(\d{2}))\s*(?:,|at)?\s*)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)?(?:\s*\(([^)]+)\))?/i.exec(tail);
  if (!absolute) return null;
  const [, monthName, dayText, yearText, isoYear, isoMonth, isoDay, hourText, minuteText, meridiem, zoneText] = absolute;
  if (!meridiem && !minuteText) return null; // a bare number is not a time
  let hour = Number(hourText);
  const minute = Number(minuteText ?? 0);
  if (meridiem) {
    if (hour < 1 || hour > 12) return null;
    hour = (hour % 12) + (meridiem.toLowerCase() === 'pm' ? 12 : 0);
  }
  if (hour > 23 || minute > 59) return null;
  const zone = zoneText && zoneOk(zoneText.trim()) ? zoneText.trim() : localZone;

  const today = partsIn(now, zone);
  if (monthName || isoMonth) {
    const month = monthName ? MONTHS.indexOf(monthName.toLowerCase()) : Number(isoMonth) - 1;
    const day = monthName ? Number(dayText) : Number(isoDay);
    const year = Number(yearText ?? isoYear ?? today.year);
    let at = inZone(year, month, day, hour, minute, zone);
    if (!yearText && !isoYear && at < now - 3_600_000) at = inZone(year + 1, month, day, hour, minute, zone);
    return at;
  }
  let at = inZone(today.year, today.month, today.day, hour, minute, zone);
  if (at <= now) at = inZone(today.year, today.month, today.day + 1, hour, minute, zone);
  return at;
}
