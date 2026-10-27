import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import source from '../assets/bip39-english.txt?raw';
import { diceWord, rollBit, appendRoll, undoRoll, resetRolls, wordlist, worksheet } from './dice';
describe('physical dice mapping', () => {
    it('matches the official bundled wordlist fingerprint', () => {
        expect(wordlist).toHaveLength(2048);
        expect(createHash('sha256').update(source).digest('hex')).toBe(
            '2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda',
        );
    });
    it.each([
        [0, 'abandon'],
        [1, 'ability'],
        [2047, 'zoo'],
    ])('maps index %i to %s', (index, word) => {
        const rolls = (index as number)
            .toString(2)
            .padStart(11, '0')
            .split('')
            .map((bit) => (bit === '1' ? 6 : 1));
        expect(diceWord({ rolls })).toMatchObject({ index, word, position: (index as number) + 1 });
    });
    it('maps all six die values, rejects invalid values and counts', () => {
        expect([1, 2, 3, 4, 5, 6].map((roll) => rollBit({ roll }))).toEqual([0, 0, 0, 1, 1, 1]);
        for (const roll of [0, 7, -1, 1.5, NaN, Infinity]) expect(() => rollBit({ roll })).toThrow();
        for (const count of [0, 1, 10, 12, 256]) expect(() => diceWord({ rolls: Array(count).fill(1) })).toThrow();
    });
    it('matches every one of the 2048 English indices', () => {
        for (let index = 0; index < 2048; index++) {
            const rolls = index
                .toString(2)
                .padStart(11, '0')
                .split('')
                .map((bit) => (bit === '1' ? 4 : 3));
            expect(diceWord({ rolls })).toMatchObject({ index, word: wordlist[index], position: index + 1 });
        }
    });
    it('supports undo and reset without changing the original rolls', () => {
        const rolls = [1, 2, 3];
        expect(appendRoll({ rolls, roll: 6, limit: 11 })).toEqual([1, 2, 3, 6]);
        expect(undoRoll({ rolls })).toEqual([1, 2]);
        expect(resetRolls()).toEqual([]);
        expect(rolls).toEqual([1, 2, 3]);
        expect(() => appendRoll({ rolls, roll: 4, limit: 3 })).toThrow();
    });
    it('shows each nonzero weight and distinguishes index from position', () => {
        const text = worksheet({ entry: diceWord({ rolls: [6, 1, 1, 1, 1, 1, 1, 1, 1, 6, 6] }) });
        expect(text).toContain('1024 + 2 + 1 = 1027');
        expect(text).toContain('Zero-based BIP39 index: 1027');
        expect(text).toContain('Printed wordlist position: 1028');
        expect(text).toContain('Verification: PASS');
    });
});
