import source from '../assets/bip39-english.txt?raw';
import { l } from './i18n';
// Only transparent integer arithmetic and public wordlist lookup. Seed generation stays in Rust.
export const wordlist = source.trim().split('\n');
export const weights = [1024, 512, 256, 128, 64, 32, 16, 8, 4, 2, 1];
export interface DiceWord {
    rolls: number[];
    bits: string;
    index: number;
    position: number;
    word: string;
    verified: boolean;
}
export const rollBit = ({ roll }: { roll: number }): number => {
    if (!Number.isInteger(roll) || roll < 1 || roll > 6) throw new Error(l.invalidRoll);
    return roll <= 3 ? 0 : 1;
};
export const appendRoll = ({ rolls, roll, limit }: { rolls: number[]; roll: number; limit: number }): number[] => {
    rollBit({ roll });
    if (rolls.length >= limit) throw new Error(l.tooManyRolls);
    return [...rolls, roll];
};
export const undoRoll = ({ rolls }: { rolls: number[] }): number[] => rolls.slice(0, -1);
export const resetRolls = (): number[] => [];
export const diceWord = ({ rolls }: { rolls: number[] }): DiceWord => {
    if (rolls.length !== 11) throw new Error(l.elevenRolls);
    const bits = rolls.map((roll) => rollBit({ roll })).join('');
    const index = rolls.reduce((sum, roll, i) => sum + rollBit({ roll }) * weights[i], 0);
    return { rolls: [...rolls], bits, index, position: index + 1, word: wordlist[index], verified: false };
};
export const manualWord = ({ word }: { word: string }): DiceWord => {
    const index = wordlist.indexOf(word.trim().toLowerCase());
    if (index === -1) throw new Error(l.invalidWord);
    return {
        rolls: [],
        bits: index.toString(2).padStart(11, '0'),
        index,
        position: index + 1,
        word: wordlist[index],
        verified: false,
    };
};
export const worksheet = ({ entry }: { entry: DiceWord }): string => {
    const terms = weights.filter((_, i) => entry.bits[i] === '1');
    const verified = entry.bits.length === 11 && parseInt(entry.bits, 2) === entry.index && wordlist[entry.index] === entry.word;
    return [
        l.verificationTitle,
        '-----------------------',
        '',
        l.diceRolls + ': ' + (entry.rolls.length ? entry.rolls.join(' ') : l.manualEntry),
        l.binaryConversion + ': ' + entry.bits.split('').join(' '),
        '',
        l.binaryValue + ': ' + entry.bits,
        '',
        l.bitWeights + ':',
        weights.join(' '),
        '',
        l.decimalCalculation + ':',
        (terms.length ? terms.join(' + ') : '0') + ' = ' + entry.index,
        '',
        l.zeroIndex + ': ' + entry.index,
        l.printPosition + ': ' + entry.position,
        l.englishWord + ': ' + entry.word,
        '',
        l.verification + ': ' + (verified ? 'PASS' : 'FAIL'),
    ].join('\n');
};
