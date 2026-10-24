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
export type Question = { header?: string; question: string; options: QuestionOption[]; multiSelect?: boolean };
/** What the user chose for one question: labels of options, and text of their own. */
export type Answer = { picked: string[]; text: string };

const FENCE = /```(?:questions|agentdeck-questions|agentdeck-ask)[^\n]*\n([\s\S]*?)```/;

/** The questions in an answer (when it has a valid block) and the text around them. */
export function parseQuestions(text: string): { questions: Question[]; rest: string } {
  const found = FENCE.exec(text);
  if (!found) return { questions: [], rest: text };
  try {
    const data = JSON.parse(found[1]) as { questions?: unknown } | unknown[];
    const list = (Array.isArray(data) ? data : (data as { questions?: unknown }).questions) as unknown[];
    const questions = (Array.isArray(list) ? list : [])
      .map((item) => item as Partial<Question> & { options?: unknown[]; multi?: boolean })
      .filter((item) => typeof item.question === 'string' && item.question.trim())
      .map((item) => ({
        header: typeof item.header === 'string' ? item.header : undefined,
        question: item.question!.trim(),
        multiSelect: item.multiSelect === true || item.multi === true,
        options: (Array.isArray(item.options) ? item.options : [])
          .map((option) => (typeof option === 'string' ? { label: option } : (option as QuestionOption)))
          .filter((option) => option && typeof option.label === 'string' && option.label.trim())
          .slice(0, 6),
      }))
      .slice(0, 6);
    if (!questions.length) return { questions: [], rest: text };
    return { questions, rest: (text.slice(0, found.index) + text.slice(found.index + found[0].length)).trim() };
  } catch {
    // A block that is not valid JSON stays as text: the user still reads the questions.
    return { questions: [], rest: text };
  }
}

/** While the answer streams, a block not closed yet is hidden instead of showing half a JSON. */
export const withoutOpenBlock = (text: string) => {
  const at = text.search(/```(?:questions|agentdeck-questions|agentdeck-ask)/);
  return at >= 0 && !FENCE.test(text) ? text.slice(0, at).trimEnd() : text;
};

/** The answers as the message sent back: short, one line per question, so any agent reads them at a glance. */
export function formatAnswers(questions: Question[], answers: Answer[]): string {
  const lines = questions.map((question, index) => {
    const answer = answers[index] ?? { picked: [], text: '' };
    const parts = [...answer.picked, ...(answer.text.trim() ? [answer.text.trim()] : [])];
    return `${index + 1}. ${question.question}\n   → ${parts.length ? parts.join('; ') : '(no preference, decide yourself)'}`;
  });
  return `My answers:\n${lines.join('\n')}`;
}

/** Told to an agent so it asks this way instead of guessing (flows, and chats whose agent supports no questions). */
export const QUESTION_RULE =
  'Unclear request where a wrong guess wastes work: before planning or editing, ask the user: one why-line, then ```agentdeck-ask {"questions":[{"id":"","header":"<=12ch","question":"?","multi":false,"options":[{"label":"1-5 words (Recommended)","description":""}]}]} ``` (1-3 questions, 2-4 options, best first, no "Other"), then stop. Never ask what code or sense can answer.';
