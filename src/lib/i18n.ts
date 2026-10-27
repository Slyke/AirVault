import strings from './en-US.json';
export const l: Record<string, string> = Object.fromEntries(
    Object.entries(strings).map(([key, value]) => [key.replace('airvault-', ''), value]),
);
