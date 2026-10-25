import { inTauri } from './history';

/** Requests native OS notification permission while the user is in Settings. */
export async function requestSystemNotificationPermission(): Promise<boolean> {
  if (!inTauri()) return false;
  try {
    const notifications = await import('@tauri-apps/plugin-notification');
    if (await notifications.isPermissionGranted()) return true;
    return (await notifications.requestPermission()) === 'granted';
  } catch {
    return false;
  }
}

/** Sends a notification through Tauri so it reaches Windows, macOS, or Linux. */
export async function sendSystemNotification(title: string, body: string): Promise<void> {
  if (!inTauri()) return;
  try {
    const notifications = await import('@tauri-apps/plugin-notification');
    if (await notifications.isPermissionGranted()) notifications.sendNotification({ title, body });
  } catch {
    // System notifications are optional; the session itself remains available in the app.
  }
}
