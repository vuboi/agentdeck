import { inTauri } from './history';

type Plugin = typeof import('@tauri-apps/plugin-notification');

/** Windows gets a toast with the AgentDeck logo (src-tauri/src/notify.rs); macOS and Linux show the app icon through
 * the plugin. A toast that fails falls back to the plugin too. */
async function notify(notifications: Plugin, title: string, body: string) {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    if (await invoke<boolean>('system_notify', { title, body })) return;
  } catch {
    // The plugin below still reaches the system.
  }
  notifications.sendNotification({ title, body });
}

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

/** Sends a sample notification now; answers why it could not be sent, or null when it went to the system. */
export async function testSystemNotification(): Promise<string | null> {
  if (!inTauri()) return 'Notifications work only in the desktop app.';
  try {
    const notifications = await import('@tauri-apps/plugin-notification');
    if (!(await notifications.isPermissionGranted()) && (await notifications.requestPermission()) !== 'granted')
      return 'The system did not allow notifications for AgentDeck. Allow them in the system settings, then try again.';
    await notify(notifications, 'AgentDeck: test', 'Notifications work. A finished task will look like this.');
    return null;
  } catch (error) {
    return String(error);
  }
}

/** Sends a notification through Tauri so it reaches Windows, macOS, or Linux. */
export async function sendSystemNotification(title: string, body: string): Promise<void> {
  if (!inTauri()) return;
  try {
    const notifications = await import('@tauri-apps/plugin-notification');
    if (await notifications.isPermissionGranted()) await notify(notifications, title, body);
  } catch {
    // System notifications are optional; the session itself remains available in the app.
  }
}
