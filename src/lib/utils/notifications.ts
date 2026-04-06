/**
 * Native OS notification utility for JARVIS.
 *
 * Sends notifications only when the app window is NOT focused (background),
 * using the Tauri 2 notification plugin.
 */
import {
  sendNotification,
  isPermissionGranted,
  requestPermission,
} from '@tauri-apps/plugin-notification';

let permissionChecked = false;
let permitted = false;

async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return permitted;
  try {
    permitted = await isPermissionGranted();
    if (!permitted) {
      const result = await requestPermission();
      permitted = result === 'granted';
    }
    permissionChecked = true;
  } catch (e) {
    console.warn('notifications: permission check failed', e);
    permitted = false;
  }
  return permitted;
}

/**
 * Send a native OS notification, but only when the app is not focused.
 * On first call, requests permission if not yet granted.
 */
export async function sendNativeNotification(
  title: string,
  body: string,
): Promise<void> {
  // Skip if the app window is currently focused
  if (document.hasFocus()) return;

  const ok = await ensurePermission();
  if (!ok) return;

  try {
    sendNotification({ title, body });
  } catch (e) {
    console.warn('notifications: failed to send native notification', e);
  }
}
