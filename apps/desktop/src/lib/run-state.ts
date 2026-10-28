// The state of one chat's run as a pure reducer: `step(state, event)` returns the next state and the transitions to show.
// No I/O, no clock, no store. Modelled on aionui-session's reducer (AionCore, reducer.rs), with the rules that matter here:
//   - Idle and Error are absorbing: only a new turn leaves them; late events of a finished turn are ignored.
//   - A stale turn (an epoch that does not advance) is dropped, so an old event never reopens a finished run.
//   - Cancel goes to Idle, never to Error: a stopped run is not a crashed one, and the process kill that follows is absorbed.
//   - A process that ends while starting, or mid-turn before a result, is Error (crashed).
//   - Waiting for the user (approval, sign-in, a question) is a count next to Running, not a state of its own: the three
//     counts are separate so the UI can tell them apart, and the run returns to plain Running when all are back to zero.
// `canSend` and `canQueue` are the only places that decide whether the input box accepts a message during a run.

export type WaitKind = 'approval' | 'auth' | 'question';
export type Waiting = Record<WaitKind, number>;

export type RunState =
  | { kind: 'idle' }
  | { kind: 'starting'; epoch: number }
  | { kind: 'running'; epoch: number; substantive: boolean; waiting: Waiting }
  | { kind: 'error'; reason: 'crashed' | 'empty' | 'backend'; message: string };

export type RunEvent =
  | { type: 'spawn'; epoch: number }
  | { type: 'turnStarted'; epoch: number }
  | { type: 'output'; text?: string }
  | { type: 'waitStart'; kind: WaitKind }
  | { type: 'waitEnd'; kind: WaitKind }
  | { type: 'turnResult'; isError: boolean; message?: string }
  | { type: 'cancel' }
  | { type: 'detached' };

export type Transition = { from: RunState; to: RunState };

export const IDLE: RunState = { kind: 'idle' };
const NO_WAIT: Waiting = { approval: 0, auth: 0, question: 0 };

const waitingTotal = (waiting: Waiting) => waiting.approval + waiting.auth + waiting.question;

/** The phase the UI cares about: a change of phase is a transition; a change inside a phase (output arriving) is not. */
function phaseOf(state: RunState): string {
  if (state.kind === 'running') return waitingTotal(state.waiting) > 0 ? 'requires-action' : 'running';
  return state.kind;
}

function settle(from: RunState, to: RunState): { state: RunState; transitions: Transition[] } {
  return { state: to, transitions: phaseOf(from) === phaseOf(to) ? [] : [{ from, to }] };
}

const unchanged = (state: RunState) => ({ state, transitions: [] as Transition[] });

export function step(state: RunState, event: RunEvent): { state: RunState; transitions: Transition[] } {
  // A new turn is the only event that leaves a finished run.
  if (event.type === 'spawn' && (state.kind === 'idle' || state.kind === 'error')) {
    return settle(state, { kind: 'starting', epoch: event.epoch });
  }
  if (event.type === 'turnStarted') {
    if (state.kind === 'running' && event.epoch <= state.epoch) return unchanged(state); // stale or duplicate
    const epoch = event.epoch;
    const next: RunState = { kind: 'running', epoch, substantive: false, waiting: { ...NO_WAIT } };
    return settle(state, next);
  }
  // Absorbing: a finished run ignores everything else.
  if (state.kind === 'idle' || state.kind === 'error') return unchanged(state);

  if (event.type === 'cancel') return settle(state, IDLE);

  if (event.type === 'detached') {
    // Starting or running with no result yet: the process is gone, so the run crashed.
    return settle(state, { kind: 'error', reason: 'crashed', message: 'The agent process ended before it finished.' });
  }

  if (state.kind === 'starting') {
    if (event.type === 'turnResult' && event.isError) {
      // A bad resume fails on the first frame: the message is the cause, so it must reach the error.
      return settle(state, { kind: 'error', reason: 'backend', message: event.message ?? '' });
    }
    if (event.type === 'output' || event.type === 'turnResult') {
      // The first frame: the run is really going.
      const running: RunState = { kind: 'running', epoch: state.epoch, substantive: false, waiting: { ...NO_WAIT } };
      return step(running, event);
    }
    return unchanged(state);
  }

  // state.kind === 'running'
  switch (event.type) {
    case 'output': {
      // Only text that is really there counts as output (a thinking-only stream does not).
      const substantive = state.substantive || (event.text ?? '').length > 0;
      return { state: { ...state, substantive }, transitions: [] };
    }
    case 'waitStart': {
      const waiting = { ...state.waiting, [event.kind]: state.waiting[event.kind] + 1 };
      return settle(state, { ...state, waiting });
    }
    case 'waitEnd': {
      const waiting = { ...state.waiting, [event.kind]: Math.max(0, state.waiting[event.kind] - 1) };
      return settle(state, { ...state, waiting });
    }
    case 'turnResult': {
      if (event.isError) return settle(state, { kind: 'error', reason: 'backend', message: event.message ?? '' });
      // A turn that finished with nothing written is an error, not a quiet success.
      if (!state.substantive) return settle(state, { kind: 'error', reason: 'empty', message: '' });
      return settle(state, IDLE);
    }
    default:
      return unchanged(state);
  }
}

/** The one unlock decision: a new message is sent at once only when the run is idle. */
export const canSend = (state: RunState) => state.kind === 'idle';

/** Whether the input box may take a message while a run goes on (queued, not sent at once). Needs a backend that accepts
 *  input during a turn, and no wait for the user: a run that is waiting for an answer cannot take a queued message. */
export const canQueue = (state: RunState, acceptsInputDuringTurn: boolean) =>
  acceptsInputDuringTurn && state.kind === 'running' && waitingTotal(state.waiting) === 0;
