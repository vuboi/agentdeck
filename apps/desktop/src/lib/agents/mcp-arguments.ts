/** Formats an MCP argument array as editable text, preserving empty values, quotes and Windows paths.
 * The result is parsed by {@link parseMcpArguments}; it is never evaluated by a shell.
 */
export function formatMcpArguments(args: readonly string[]): string {
  return args.map((arg) => arg && !/[\s"'\\]/.test(arg) ? arg : `"${arg.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`).join(' ');
}

/** Parses whitespace-separated MCP arguments with single/double quotes and no shell expansion.
 * Backslashes stay literal except before a double quote or backslash inside double quotes.
 * @throws {Error} If a quoted argument is not closed.
 */
export function parseMcpArguments(text: string): string[] {
  const args: string[] = [];
  let value = '';
  let quote = '';
  let started = false;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (quote) {
      if (char === quote) quote = '';
      else if (quote === '"' && char === '\\' && (text[index + 1] === '"' || text[index + 1] === '\\')) value += text[++index];
      else value += char;
    } else if (char === '"' || char === "'") {
      quote = char;
      started = true;
    } else if (/\s/.test(char)) {
      if (started) args.push(value);
      value = '';
      started = false;
    } else {
      value += char;
      started = true;
    }
  }
  if (quote) throw new Error('Close the quoted argument with a matching quote.');
  if (started) args.push(value);
  return args;
}
