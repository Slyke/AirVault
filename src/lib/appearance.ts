export const palettes = ['d4rk', 'terminal', 'paper', 'ocean'] as const;
export type Palette = (typeof palettes)[number];
export interface Appearance {
    palette: Palette;
    theme: 'dark' | 'light';
    font: string;
    scale: number;
}
export const normalizeScale = ({ scale }: { scale: number }) =>
    Number.isFinite(scale) ? Math.max(70, Math.min(180, Math.round(scale / 10) * 10)) : 100;
export const readAppearance = (): Appearance => {
    const result: Appearance = { palette: 'd4rk', theme: 'dark', font: 'system-stack', scale: 100 };
    try {
        const palette = localStorage.getItem('d4rk-styleguide-palette');
        const theme = localStorage.getItem('d4rk-styleguide-theme');
        const font = localStorage.getItem('d4rk-styleguide-font');
        const scale = localStorage.getItem('d4rk-styleguide-scale');
        if (palettes.includes(palette as Palette)) result.palette = palette as Palette;
        if (theme === 'dark' || theme === 'light') result.theme = theme;
        if (font && ['system-stack', 'consolas', 'cascadia-mono'].includes(font)) result.font = font;
        if (scale !== null) result.scale = normalizeScale({ scale: Number(scale) });
    } catch {
        /* A denied preference store must not prevent using the app. */
    }
    return result;
};
let activeAppearance: Appearance | undefined;
export const currentAppearance = () => ({ ...(activeAppearance ?? readAppearance()) });
export const applyAppearance = ({ appearance }: { appearance: Appearance }) => {
    const next = { ...appearance, scale: normalizeScale({ scale: appearance.scale }) };
    activeAppearance = next;
    Object.assign(document.documentElement.dataset, { palette: next.palette, theme: next.theme, font: next.font });
    document.documentElement.style.setProperty('--ui-scale', String(next.scale / 100));
    document.documentElement.style.zoom = String(next.scale / 100);
    try {
        for (const key of ['palette', 'theme', 'font', 'scale'] as const)
            localStorage.setItem('d4rk-styleguide-' + key, String(next[key]));
    } catch {
        /* Preferences are optional; no vault data ever enters this store. */
    }
    window.dispatchEvent(new Event('airvault-appearance'));
};
export const scaleShortcut = ({ event }: { event: KeyboardEvent }) => {
    if (!event.ctrlKey || event.altKey) return;
    let delta = 0;
    if (['+', '='].includes(event.key) || event.code === 'NumpadAdd') delta = 10;
    else if (event.key === '-' || event.code === 'NumpadSubtract') delta = -10;
    else if (event.key !== '0') return;
    event.preventDefault();
    const appearance = currentAppearance();
    applyAppearance({ appearance: { ...appearance, scale: delta ? appearance.scale + delta : 100 } });
};
