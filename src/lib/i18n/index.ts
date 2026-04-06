// JARVIS - Internationalization (i18n) system
// Lightweight store-based translation with Spanish and English support
import { writable, derived, get } from 'svelte/store';
import { es } from './es';
import { en } from './en';

export type Locale = 'es' | 'en';
export type TranslationDict = Record<string, string>;

const STORAGE_KEY = 'jarvis-locale';

function getInitialLocale(): Locale {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === 'en' || saved === 'es') return saved;
  }
  return 'es';
}

const dictionaries: Record<Locale, TranslationDict> = { es, en };

export const locale = writable<Locale>(getInitialLocale());

// Persist locale changes
locale.subscribe((val) => {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, val);
  }
});

export const translations = derived(locale, ($locale) => dictionaries[$locale]);

/**
 * Translate a key. Use in components as: t('key') or t('key', { count: 3 })
 * Supports simple {var} interpolation.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const dict = dictionaries[get(locale)];
  let text = dict[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

/**
 * Reactive translation function - returns a derived store.
 * Use in templates as: {$tr('key')} or {$tr('key', { n: 5 })}
 */
export const tr = derived(locale, ($locale) => {
  const dict = dictionaries[$locale];
  return (key: string, params?: Record<string, string | number>): string => {
    let text = dict[key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
      }
    }
    return text;
  };
});

export function setLocale(l: Locale) {
  locale.set(l);
}

export function toggleLocale() {
  locale.update(l => l === 'es' ? 'en' : 'es');
}
