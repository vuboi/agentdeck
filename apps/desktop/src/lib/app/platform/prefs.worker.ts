// The prefs database runs here, off the UI thread: reads, batched writes, and the parse of stored values (see prefs-db.ts).
// The main thread (prefs.ts) sends one request at a time by id and gets its JSON text back.

import { handle, type PrefsRequest } from './prefs-db';

type Message = PrefsRequest & { id: number };

self.onmessage = (event: MessageEvent<Message>) => {
  const { id, ...request } = event.data;
  handle(request as PrefsRequest).then(
    (result) => self.postMessage({ id, result }),
    (error: unknown) => self.postMessage({ id, error: String(error) }),
  );
};
