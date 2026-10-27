// One confirm box for the Store (a ZModal drawn by ConfirmHost), in place of the browser's window.confirm.
// `askConfirm` resolves true on OK and false when the box is closed any other way.

export type ConfirmRequest = { title: string; text: string; ok?: string; danger?: boolean };

export const confirmBox = $state({
  open: false,
  request: null as ConfirmRequest | null,
});

let settle: ((answer: boolean) => void) | null = null;

export function askConfirm(request: ConfirmRequest): Promise<boolean> {
  // A box still open is answered "no" before the new one takes its place.
  settle?.(false);
  confirmBox.request = request;
  confirmBox.open = true;
  return new Promise((resolve) => (settle = resolve));
}

/** Called by ConfirmHost: OK, or the box closed (Cancel, Escape, backdrop). */
export function answerConfirm(answer: boolean) {
  const done = settle;
  settle = null;
  confirmBox.open = false;
  done?.(answer);
}
