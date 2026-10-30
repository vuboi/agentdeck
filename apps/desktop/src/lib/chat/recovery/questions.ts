// Questions an agent asks the user before it goes on, as in Claude's and Codex's own apps: a few questions, each with
// options to pick (or text of your own). Any agent can ask them: it writes one fenced block in its answer,
//
//   ```agentdeck-ask
//   {"questions":[{"id":"scope","header":"Scope","question":"Which pages get the button?","multi":false,
//     "options":[{"label":"Report page only (Recommended)","description":"…"},{"label":"Every page with an export"}]}]}
//   ```
//
// (the shape is the common part of Claude's AskUserQuestion and Codex's request_user_input; `questions` as the fence
// name and `multiSelect` are read too)
//
// and stops there. The chat shows the questions as cards, one at a time, and sends the answers back as the next message.

export type QuestionOption = { label: string; description?: string };
export type Question = { header?: string; question: string; options: QuestionOption[]; multiSelect?: boolean; /** The agent cannot go on without the answer: skipping it does not let the agent decide for itself. */ required?: boolean };
/** One question with one choice among two or three options (or one): the chat shows it as a quick card, a click answers. */
export const isQuick = (questions: Question[]) =>
  questions.length === 1 && !questions[0].multiSelect && questions[0].options.length > 0 && questions[0].options.length <= 3;
/** What the user chose for one question: labels of options, and text of their own. */
export type Answer = { picked: string[]; text: string };

const FENCE = /```(?:questions|agentdeck-questions|agentdeck-ask)[^\n]*\n([\s\S]*?)```/;

/** A block written without its fence: a `{"questions":…}` object that ends the answer (some models drop the fence). */
function bareBlock(text: string): { body: string; index: number; length: number } | null {
  const index = text.search(/\{\s*"questions"\s*:[^]*$/);
  if (index < 0) return null;
  const body = text.slice(index).trim();
  return body.endsWith('}') ? { body, index, length: text.length - index } : null;
}

/** The questions in an answer (when it has a valid block) and the text around them. */
export function parseQuestions(text: string): { questions: Question[]; rest: string } {
  const found = FENCE.exec(text);
  const block = found ? { body: found[1], index: found.index, length: found[0].length } : bareBlock(text);
  if (!block) return { questions: [], rest: text };
  try {
    const data = JSON.parse(block.body) as { questions?: unknown } | unknown[];
    const list = (Array.isArray(data) ? data : (data as { questions?: unknown }).questions) as unknown[];
    // Each provider names the same parts its own way (question / text / prompt, options / choices, label / title / value,
    // multi / multiSelect / multiple): all of them land in one shape here.
    const questions = (Array.isArray(list) ? list : [])
      .map((item) => item as Record<string, unknown>)
      .map((item) => ({
        header: typeof item.header === 'string' ? item.header : undefined,
        question: [item.question, item.text, item.prompt].find((value): value is string => typeof value === 'string')?.trim() ?? '',
        multiSelect: item.multiSelect === true || item.multi === true || item.multiple === true,
        required: item.required === true,
        options: (Array.isArray(item.options) ? item.options : Array.isArray(item.choices) ? item.choices : [])
          .map((option): QuestionOption => {
            if (typeof option === 'string') return { label: option };
            const entry = option as Record<string, unknown>;
            const label = [entry.label, entry.title, entry.value].find((value): value is string => typeof value === 'string') ?? '';
            return { label, description: typeof entry.description === 'string' ? entry.description : undefined };
          })
          .filter((option) => option.label.trim())
          .slice(0, 6),
      }))
      .filter((item) => item.question)
      .slice(0, 6);
    if (!questions.length) return { questions: [], rest: text };
    return { questions, rest: (text.slice(0, block.index) + text.slice(block.index + block.length)).trim() };
  } catch {
    // A block that is not valid JSON stays as text: the user still reads the questions.
    return { questions: [], rest: text };
  }
}

/** The questions of an answer as one finished step, for the list of steps under the reply: its command line is the first
 *  question, its output all of them with their options. Null when the answer asks nothing. */
export function askAsStep(text: string): { id: string; command: string; output: string } | null {
  const { questions } = parseQuestions(text);
  if (!questions.length) return null;
  const output = questions
    .map((question, index) => {
      const options = question.options.map((option, at) => `   ${at + 1}. ${option.label}${option.description ? `: ${option.description}` : ''}`);
      const mark = [question.multiSelect ? 'pick any' : '', question.required ? 'required' : ''].filter(Boolean).join(', ');
      return [`${index + 1}. ${question.question}${mark ? ` (${mark})` : ''}`, ...options].join('\n');
    })
    .join('\n\n');
  let hash = 0;
  for (const char of output) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return { id: `ask-${(hash >>> 0).toString(36)}`, command: `AskUser  ${questions[0].question}`, output };
}

/** While the answer streams, a block not closed yet is hidden instead of showing half a JSON. */
export const withoutOpenBlock = (text: string) => {
  const at = text.search(/```(?:questions|agentdeck-questions|agentdeck-ask)/);
  return at >= 0 && !FENCE.test(text) ? text.slice(0, at).trimEnd() : text;
};

/** An answer as it is read in the chat: the questions block never shows as text, whether it is still being written (not
 *  closed yet), complete but with more text after it, or finished. The questions have their own card and step. */
export function withoutQuestions(text: string): string {
  const { questions, rest } = parseQuestions(text);
  return withoutOpenBlock(questions.length ? rest.replace(/\n{3,}/g, '\n\n') : text);
}

/** The answers as the message sent back: short, one line per question, so any agent reads them at a glance. */
export function formatAnswers(questions: Question[], answers: Answer[]): string {
  const lines = questions.map((question, index) => {
    const answer = answers[index] ?? { picked: [], text: '' };
    const parts = [...answer.picked, ...(answer.text.trim() ? [answer.text.trim()] : [])];
    // An ordinary question left open is a preference the agent may settle; a required one is not.
    const none = question.required ? '(not answered: this one is required, do not guess it)' : '(no preference, decide yourself)';
    return `${index + 1}. ${question.question}\n   → ${parts.length ? parts.join('; ') : none}`;
  });
  return `My answers:\n${lines.join('\n')}`;
}

/** Told to an agent so it asks this way instead of guessing (flows, and chats whose agent supports no questions). */
export const QUESTION_RULE =
  'Unclear request where a wrong guess wastes work: before planning or editing, ask the user: one why-line, then ```agentdeck-ask {"questions":[{"id":"","header":"<=12ch","question":"?","multi":false,"options":[{"label":"1-5 words (Recommended)","description":""}]}]} ``` (1-3 questions, 2-4 options, best first, no "Other"), then stop. Never ask what code or sense can answer.';
