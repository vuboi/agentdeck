// What an answer becomes before it is read aloud: markdown turned into plain sentences. Pure, so node can test it.

/** Plain words from markdown: code, links, addresses and symbols are not read out. `paths` turns the file paths of a text
 *  into their names (path-links.ts; passed in so this file stays free of imports a plain node run cannot resolve). */
export function speakable(markdown: string, paths: (text: string) => string = (text) => text): string {
  let text = markdown.replace(/\r/g, '');
  text = text.replace(/```[\s\S]*?(```|$)/g, '\n');
  text = text.replace(/~~~[\s\S]*?(~~~|$)/g, '\n');
  text = paths(text);
  text = text
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>\n]+>/g, ' ')
    .replace(/https?:\/\/\S+/g, ' ')
    // `short words` are kept, `code(like, this)` is not.
    .replace(/`([^`\n]*)`/g, (_, code: string) => (/^[\p{L}\p{N} ]{1,40}$/u.test(code) ? code : ' '))
    .replace(/^\s{0,3}#{1,6}\s+/gm, '')
    .replace(/^\s*>\s?/gm, '')
    .replace(/^\s*([-*+]|\d+[.)])\s+/gm, '')
    .replace(/^\s*\|?[\s:|-]+\|?\s*$/gm, '')
    .replace(/\|/g, ', ')
    .replace(/(\*\*|__|\*|_|~~)/g, '')
    .replace(/\p{Extended_Pictographic}️?/gu, '')
    .replace(/[ \t]+/g, ' ');
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /[\p{L}\p{N}]/u.test(line))
    .join('\n');
}

/** The text cut into pieces of at most `max` characters, at the ends of sentences where it can: each piece is one call to the voice. */
export function sentences(text: string, max = 240): string[] {
  const out: string[] = [];
  for (const line of text.split('\n')) {
    let current = '';
    for (const piece of line.split(/(?<=[.!?…;:])\s+/)) {
      if (!piece) continue;
      if (current && current.length + 1 + piece.length > max) {
        out.push(current);
        current = '';
      }
      if (piece.length > max) {
        // A very long sentence: cut at commas, then at spaces.
        let rest = piece;
        while (rest.length > max) {
          const cut = Math.max(rest.lastIndexOf(', ', max - 2), rest.lastIndexOf(' ', max - 1));
          const at = cut > max / 3 ? cut + 1 : max;
          out.push(rest.slice(0, at).trim());
          rest = rest.slice(at).trim();
        }
        current = rest;
      } else current = current ? `${current} ${piece}` : piece;
    }
    if (current) out.push(current);
  }
  return out.filter((piece) => /[\p{L}\p{N}]/u.test(piece));
}

const VIETNAMESE = /[àáảãạăằắẳẵặâầấẩẫậèéẻẽẹêềếểễệìíỉĩịòóỏõọôồốổỗộơờớởỡợùúủũụưừứửữựỳýỷỹỵđ]/gi;

// A word with no Vietnamese mark is Vietnamese only if it can be one syllable (an optional start, vowels, an optional end):
// "memory", "agent", "deck", "check", "pnpm" cannot, so they are English even inside a Vietnamese sentence.
const SYLLABLE = /^(ngh|ng|nh|ch|gh|gi|kh|ph|th|tr|qu|[bcdghklmnpqrstvx])?[aeiouy]{1,3}(ch|ng|nh|c|m|n|p|t)?$/;

/** The word is English: only plain letters, and it cannot be a Vietnamese syllable. */
function englishWord(word: string): boolean {
  const letters = word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '').toLowerCase();
  return /^[a-z]{2,}$/.test(letters) && !SYLLABLE.test(letters);
}

/** A sentence cut into runs by language, so English terms inside Vietnamese are said by an English voice instead of being
 *  read the Vietnamese way. A sentence with no Vietnamese mark at all is English as a whole. */
export function segments(sentence: string): { text: string; vi: boolean }[] {
  if (!isVietnamese(sentence)) return [{ text: sentence, vi: false }];
  const out: { text: string; vi: boolean }[] = [];
  for (const token of sentence.split(/\s+/).filter(Boolean)) {
    const vi = !englishWord(token);
    const last = out[out.length - 1];
    if (last && last.vi === vi) last.text += ` ${token}`;
    else out.push({ text: token, vi });
  }
  return out;
}

/** Whether a sentence is Vietnamese (read by Piper) rather than English (read by the system's own voice). */
export const isVietnamese = (text: string) => (text.match(VIETNAMESE)?.length ?? 0) >= 2;
