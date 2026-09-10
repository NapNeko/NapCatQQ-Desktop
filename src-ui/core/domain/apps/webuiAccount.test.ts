import { describe, expect, it } from 'vitest';
import { validateWebUiPassword, validateWebUiUsername } from './webuiAccount';

describe('validateWebUiPassword', () => {
    it('empty means auto-generate, no error', () => {
        expect(validateWebUiPassword('')).toBeNull();
    });

    it('mirrors upstream rules', () => {
        expect(validateWebUiPassword('Ab1')).toMatch(/8 位/);
        expect(validateWebUiPassword('abcdefgh1')).toMatch(/大写/);
        expect(validateWebUiPassword('ABCDEFGH1')).toMatch(/小写/);
        expect(validateWebUiPassword('Abcdefghi')).toMatch(/数字/);
        expect(validateWebUiPassword('Abcdefg1')).toBeNull();
    });
});

describe('validateWebUiUsername', () => {
    it('rejects whitespace only', () => {
        expect(validateWebUiUsername('')).toBeNull();
        expect(validateWebUiUsername('ops')).toBeNull();
        expect(validateWebUiUsername('o ps')).not.toBeNull();
    });
});
