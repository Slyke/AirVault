<script lang="ts">
    import { onMount } from 'svelte';
    import { call, errorText, type AppInfo } from './api';
    import { l } from './i18n';
    import Info from './Info.svelte';
    let info = $state<AppInfo | null>(null);
    let busy = $state(false);
    let error = $state('');
    let copied = $state(false);
    let expected = $state('');
    const normalized = $derived(expected.trim().toLowerCase());
    const valid = $derived(/^[a-f0-9]{64}$/.test(normalized));
    const refresh = async () => {
        busy = true;
        error = '';
        copied = false;
        try {
            info = await call<AppInfo>({ command: 'app_info' });
        } catch (cause) {
            info = null;
            error = errorText({ error: cause });
        } finally {
            busy = false;
        }
    };
    const copy = async () => {
        if (!info) return;
        try {
            await navigator.clipboard.writeText(info.binarySha256);
            copied = true;
        } catch {
            error = l.clipboardFailed;
        }
    };
    onMount(() => {
        void refresh();
    });
</script>

<section class="about-panel">
    <div class="heading-with-help">
        <h2>{l.applicationIdentity}</h2>
        <Info title={l.applicationIdentity} topic="integrity"><p>{l.integrityHelp}</p></Info>
    </div>
    {#if error}<p class="alert danger" role="alert">{error}</p>{/if}
    {#if busy}<p role="status">{l.hashingBinary}</p>{/if}
    {#if info}
        <dl class="about-details">
            <dt>{l.version}</dt>
            <dd>{info.version}</dd>
            <dt>{l.versionHash}</dt>
            <dd><code class="wrap" data-testid="source-hash">{info.sourceHash}</code></dd>
            <dt>{l.gitRevision}</dt>
            <dd>
                <code class="wrap">{info.buildHash}</code>{#if info.dirty}<span class="badge neutral">{l.localChanges}</span>{/if}
            </dd>
            <dt>{l.builtAt}</dt>
            <dd>{new Date(Number(info.builtAt) * 1000).toISOString()}</dd>
            <dt>{l.buildTarget}</dt>
            <dd><code class="wrap">{info.target}</code></dd>
            <dt>{l.binaryHash}</dt>
            <dd><code class="wrap" data-testid="binary-hash">{info.binarySha256}</code></dd>
        </dl>
        <div class="actions">
            <button disabled={busy} onclick={() => void copy()}>{copied ? l.copiedHash : l.copyHash}</button><button
                disabled={busy}
                onclick={() => void refresh()}>{l.recalculateHash}</button
            >
        </div>
        <details><summary>{l.binaryLocation}</summary><code class="wrap">{info.binaryPath}</code></details>
        <label for="expected-hash">{l.compareHash}</label>
        <input
            id="expected-hash"
            bind:value={expected}
            spellcheck="false"
            autocomplete="off"
            maxlength="256"
            placeholder={l.compareHashHint}
        />
        {#if normalized}<p
                role="status"
                class:hash-match={valid && normalized === info.binarySha256}
                class:hash-mismatch={!valid || normalized !== info.binarySha256}
            >
                {!valid ? l.hashInvalid : normalized === info.binarySha256 ? l.hashMatches : l.hashDiffers}
            </p>{/if}
        <p class="hint">{l.integrityLimit}</p>
    {/if}
</section>
