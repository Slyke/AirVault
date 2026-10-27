import { describe, expect, it } from 'vitest';
import { passwordGrade } from './password';
import { normalizeScale } from './appearance';

describe('advisory password grading', () => {
    it('does not mistake predictable character substitutions for randomness', () => {
        const weak = passwordGrade({ password: 'Password1!' });
        expect(weak.upper && weak.lower && weak.number && weak.special).toBe(true);
        expect(weak.score).toBeLessThan(3);
        expect(passwordGrade({ password: '8nZ!v4Q#r7Lm@2Tc$9Wx' }).score).toBeGreaterThan(weak.score);
    });
    it('counts Unicode characters and accepts reporting for weak six-character input', () => {
        expect(passwordGrade({ password: '123456' }).length).toBe(6);
        expect(passwordGrade({ password: '123456' }).score).toBe(0);
        expect(passwordGrade({ password: '🦀🦀🦀' }).length).toBe(3);
    });
});
describe('interface scale bounds', () => {
    it('normalizes corrupt and out-of-range stored preferences', () => {
        expect(normalizeScale({ scale: NaN })).toBe(100);
        expect(normalizeScale({ scale: 1 })).toBe(70);
        expect(normalizeScale({ scale: 999 })).toBe(180);
        expect(normalizeScale({ scale: 124 })).toBe(120);
    });
});
