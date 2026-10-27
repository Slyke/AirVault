import { ZxcvbnFactory } from '@zxcvbn-ts/core';
import { dictionary as common, adjacencyGraphs } from '@zxcvbn-ts/language-common';
import { dictionary as english, translations } from '@zxcvbn-ts/language-en';

const estimator = new ZxcvbnFactory({ dictionary: { ...common, ...english }, graphs: adjacencyGraphs, translations });
export const passwordGrade = ({ password }: { password: string }) => {
    // Bound expensive pattern matching. Never retain, log, or transmit the result's password field.
    const score = password ? estimator.check(password.slice(0, 128), ['airvault', 'vault']).score : 0;
    return {
        score,
        length: [...password].length,
        upper: /\p{Lu}/u.test(password),
        lower: /\p{Ll}/u.test(password),
        number: /\p{N}/u.test(password),
        special: /[^\p{L}\p{N}\s]/u.test(password),
    };
};
