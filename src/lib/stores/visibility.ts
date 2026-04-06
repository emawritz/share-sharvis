import { writable } from 'svelte/store';

export const appVisible = writable(true);

export function initVisibility() {
  const onChange = () => appVisible.set(!document.hidden);
  document.addEventListener('visibilitychange', onChange);
  return () => document.removeEventListener('visibilitychange', onChange);
}
