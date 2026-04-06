import { writable, get } from 'svelte/store';
import type { Preset } from '../types';

const STORAGE_KEY = 'jarvis-presets';

const DEFAULT_PRESETS: Preset[] = [
  { name: 'Correr tests', target: 'atlas', prompt: 'Corri todos los tests y arregla los que fallen' },
  { name: 'Correr tests frontend', target: 'pixel', prompt: 'Corri ng test y arregla los que fallen' },
  { name: 'Fix lint', target: 'both', prompt: 'Corri el linter, arregla todos los errores' },
  { name: 'Git status', target: 'both', prompt: 'Hace git status y git diff --stat, contame que cambios hay pendientes' },
  { name: 'Build check', target: 'both', prompt: 'Hace un build completo y arregla cualquier error de compilacion' },
  { name: 'Implementar feature', target: 'both', prompt: 'Implementa {feature_description}. Segui buenas practicas y escribi tests.' },
  { name: 'Fix bug', target: 'both', prompt: 'Encontra y arregla este bug: {bug_description}. Explicame que lo causaba.' },
  { name: 'Revisar archivo', target: 'atlas', prompt: 'Revisa el archivo {file_path} y sugeri mejoras de calidad, seguridad y performance.' },
];

function readFromStorage(): Preset[] {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) return JSON.parse(saved);
  } catch { /* ignore */ }
  return DEFAULT_PRESETS.slice();
}

function writeToStorage(presets: Preset[]) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(presets)); } catch { /* ignore */ }
}

// Initialize localStorage with defaults if empty
if (typeof localStorage !== 'undefined' && !localStorage.getItem(STORAGE_KEY)) {
  writeToStorage(DEFAULT_PRESETS.slice());
}

export const presets = writable<Preset[]>(readFromStorage());

/** Reload presets from localStorage into the store */
export function loadPresets() {
  presets.set(readFromStorage());
}

/** Save a new preset and update the store */
export function savePreset(name: string, target: Preset['target'], prompt: string) {
  const all = readFromStorage();
  all.push({ name, target, prompt });
  writeToStorage(all);
  presets.set(all);
}

/** Delete a preset by index and update the store */
export function deletePreset(idx: number) {
  const all = readFromStorage();
  all.splice(idx, 1);
  writeToStorage(all);
  presets.set(all);
}

/** Get current presets synchronously */
export function getPresets(): Preset[] {
  return get(presets);
}

/** Extract template variable names from a prompt string */
export function extractTemplateVars(promptStr: string): string[] {
  const matches = promptStr.match(/\{(\w+)\}/g) || [];
  return [...new Set(matches.map(m => m.slice(1, -1)))];
}

/** Apply template variable substitution to a prompt string */
export function applyTemplate(template: string, vars: Record<string, string>): string {
  let result = template;
  for (const [key, val] of Object.entries(vars)) {
    result = result.replace(new RegExp(`\\{${key}\\}`, 'g'), val || `{${key}}`);
  }
  return result;
}
