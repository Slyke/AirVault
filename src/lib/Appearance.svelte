<script lang="ts">
    import { untrack, onMount } from 'svelte';
    import { applyAppearance, currentAppearance, palettes } from './appearance';
    import { l } from './i18n';
    let appearance = $state(untrack(currentAppearance));
    const apply = () => applyAppearance({ appearance });
    onMount(() => {
        const update = () => {
            appearance = currentAppearance();
        };
        window.addEventListener('airvault-appearance', update);
        return () => window.removeEventListener('airvault-appearance', update);
    });
</script>

<div class="theme-controls">
    <label
        >{l.palette}<select bind:value={appearance.palette} onchange={apply}
            >{#each palettes as palette}<option value={palette}>{l[palette]}</option>{/each}</select
        ></label
    >
    <label
        >{l.themeMode}<select bind:value={appearance.theme} onchange={apply}
            ><option value="dark">{l.dark}</option><option value="light">{l.light}</option></select
        ></label
    >
    <label
        >{l.font}<select bind:value={appearance.font} onchange={apply}
            ><option value="system-stack">System monospace</option><option value="consolas">Consolas</option><option
                value="cascadia-mono">Cascadia Mono</option
            ></select
        ></label
    >
    <label
        >{l.scale}<select bind:value={appearance.scale} onchange={apply}
            >{#each [70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180] as scale}<option value={scale}>{scale}%</option
                >{/each}</select
        ></label
    >
</div>
<small role="status">{l[appearance.palette]} / {l[appearance.theme]} / {appearance.scale}% · Ctrl + / − · Ctrl 0</small>
