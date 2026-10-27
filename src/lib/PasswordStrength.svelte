<script lang="ts">
    import { passwordGrade } from './password';
    import { l } from './i18n';
    import Info from './Info.svelte';
    let { password }: { password: string } = $props();
    const grade = $derived(passwordGrade({ password }));
</script>

<div class="password-feedback" aria-live="polite">
    <div class="strength-heading">
        <span>{l.passwordStrength}: <strong>{password ? l['strength' + grade.score] : l.strengthEmpty}</strong></span>
        <Info title={l.passwordStrength} topic="security"><p>{l.strengthHelp}</p></Info>
    </div>
    <meter min="0" max="5" value={password ? grade.score + 1 : 0} aria-label={l.passwordStrength}></meter>
    <div class="password-checks">
        {#each ['upper', 'lower', 'number', 'special'] as check}
            <span class:present={grade[check as 'upper' | 'lower' | 'number' | 'special']}
                >{grade[check as 'upper' | 'lower' | 'number' | 'special'] ? '✓' : '·'} {l['password' + check]}</span
            >
        {/each}
    </div>
    <small
        >{grade.length < 6 ? l.passwordMinimum : l.passwordMinimumMet} · {l.randomness}: {password
            ? l['strength' + grade.score]
            : '—'}</small
    >
</div>
