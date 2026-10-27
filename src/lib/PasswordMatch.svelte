<script lang="ts">
    import { l } from './i18n';
    let { password, confirmation }: { password: string; confirmation: string } = $props();
</script>

{#if confirmation}
    <small class="password-match" class:matched={password === confirmation} role="status"
        >{password === confirmation ? l.passwordsMatch : l.passwordsDiffer}</small
    >
{/if}
