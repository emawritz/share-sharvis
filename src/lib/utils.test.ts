import { describe, it, expect } from 'vitest';
import { handleError } from './utils';

describe('handleError', () => {
  it('returns the string as-is when given a plain string (Tauri error format)', () => {
    expect(handleError('connection refused')).toBe('connection refused');
  });

  it('returns Error.message when given an Error object', () => {
    expect(handleError(new Error('something went wrong'))).toBe('something went wrong');
  });

  it('converts numbers to string via String()', () => {
    expect(handleError(42)).toBe('42');
  });

  it('converts null to string', () => {
    expect(handleError(null)).toBe('null');
  });

  it('converts undefined to string', () => {
    expect(handleError(undefined)).toBe('undefined');
  });

  it('converts plain objects to string', () => {
    expect(handleError({ code: 404 })).toBe('[object Object]');
  });

  it('returns empty string when given an empty string', () => {
    expect(handleError('')).toBe('');
  });

  it('handles Error subclasses correctly', () => {
    class CustomError extends Error {
      constructor(msg: string) {
        super(msg);
        this.name = 'CustomError';
      }
    }
    expect(handleError(new CustomError('custom fail'))).toBe('custom fail');
  });
});
