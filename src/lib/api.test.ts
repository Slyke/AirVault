import { describe, expect, it } from 'vitest';
import { satoshis } from './api';
describe('Bitcoin amount entry', () => {
    it('rejects fractional, ambiguous and overflowing satoshi values', () => {
        for (const value of ['0.1', '1e3', '-1', '01', 'NaN', '2100000000000001', ' 1', '']) {
            expect(() => satoshis({ value })).toThrow();
        }
    });
    it('preserves the full integer Bitcoin monetary range', () => {
        expect(satoshis({ value: '2100000000000000' })).toBe(2100000000000000);
        expect(satoshis({ value: '10001' })).toBe(10001);
    });
});
