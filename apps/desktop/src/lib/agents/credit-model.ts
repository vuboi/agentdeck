// Claude models billed to the Claude Console's API credit: the same Claude Code CLI, run on an API key instead of the
// signed-in plan. They are the plan's models with this prefix, listed after a "Claude credit" divider in the pickers.
// Free of stores so plain node can test it.

export const CREDIT_PREFIX = 'credit:';

/** The CLI model of a credit id (`credit:claude-opus-5-5` gives `claude-opus-5-5`); null for any other id. */
export function creditModel(value: string | null | undefined): string | null {
  return value?.startsWith(CREDIT_PREFIX) ? value.slice(CREDIT_PREFIX.length) || null : null;
}

/** The model a price or a label is looked up by: the credit prefix taken off. */
export const baseModel = (value: string) => creditModel(value) ?? value;
