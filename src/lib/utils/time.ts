// JARVIS - Shared time formatting utilities
import { t } from '$lib/i18n';

/**
 * Format a number of seconds into a human-readable age string.
 * Uses i18n for localization (e.g. "hace 5m" / "5m ago").
 */
export function formatAge(secs: number): string {
  let time: string;
  if (secs < 60) {
    time = t('time.seconds', { n: secs });
  } else if (secs < 3600) {
    time = t('time.minutes', { n: Math.floor(secs / 60) });
  } else if (secs < 86400) {
    time = t('time.hours', { n: Math.floor(secs / 3600) });
  } else {
    time = t('time.days', { n: Math.floor(secs / 86400) });
  }
  return t('time.ago', { time });
}
