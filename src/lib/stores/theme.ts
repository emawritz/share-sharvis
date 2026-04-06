// JARVIS - Theme store (light/dark toggle)
import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'jarvis-theme';

function getInitialTheme(): Theme {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'light' || saved === 'dark') return saved;
  }
  return 'dark';
}

export const theme = writable<Theme>(getInitialTheme());

// Apply theme to DOM and persist
theme.subscribe((val) => {
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', val);
  }
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, val);
  }
});

export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}
