/** A request to bring one message of the open chat into view (e.g. the chat a background task came from). */
export const focusMessage = $state<{ id: string; at: number }>({ id: '', at: 0 });

export function requestFocusMessage(id: string) {
  focusMessage.id = id;
  focusMessage.at = Date.now();
}
