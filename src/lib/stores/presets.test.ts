import { describe, it, expect } from 'vitest';
import { applyTemplate, extractTemplateVars } from './presets';

describe('applyTemplate', () => {
  it('substitutes a single placeholder', () => {
    expect(applyTemplate('Hello {name}!', { name: 'World' })).toBe('Hello World!');
  });

  it('substitutes multiple different placeholders', () => {
    const result = applyTemplate('Fix {bug_description} in {file_path}', {
      bug_description: 'null pointer',
      file_path: 'src/main.ts',
    });
    expect(result).toBe('Fix null pointer in src/main.ts');
  });

  it('replaces multiple occurrences of the same placeholder', () => {
    const result = applyTemplate('{x} plus {x} equals two {x}', { x: 'one' });
    expect(result).toBe('one plus one equals two one');
  });

  it('preserves {key} when no value is provided for that key', () => {
    // Only vars explicitly passed are substituted; others stay as-is
    const result = applyTemplate('Implement {feature_description}. Fix {bug_description}.', {
      feature_description: 'dark mode',
    });
    expect(result).toBe('Implement dark mode. Fix {bug_description}.');
  });

  it('returns empty string for an empty template', () => {
    expect(applyTemplate('', { foo: 'bar' })).toBe('');
  });

  it('returns template unchanged when it has no placeholders', () => {
    const tpl = 'Run all tests and fix failures';
    expect(applyTemplate(tpl, { anything: 'value' })).toBe(tpl);
  });

  it('returns template unchanged when vars object is empty', () => {
    const tpl = 'Hello {world}';
    expect(applyTemplate(tpl, {})).toBe(tpl);
  });

  it('handles an empty string value by preserving the placeholder', () => {
    // val is '' → fallback to `{key}` per the implementation: val || `{${key}}`
    const result = applyTemplate('Hello {name}', { name: '' });
    expect(result).toBe('Hello {name}');
  });
});

describe('extractTemplateVars', () => {
  it('returns unique variable names from a template', () => {
    const vars = extractTemplateVars('Fix {bug} and {bug} in {file}');
    expect(vars).toEqual(['bug', 'file']);
  });

  it('returns empty array when no placeholders present', () => {
    expect(extractTemplateVars('No placeholders here')).toEqual([]);
  });

  it('returns empty array for an empty string', () => {
    expect(extractTemplateVars('')).toEqual([]);
  });
});
