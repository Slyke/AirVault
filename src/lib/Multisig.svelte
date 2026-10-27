<script lang="ts">
    import { open, save } from '@tauri-apps/plugin-dialog';
    import { call, type Snapshot, type Network, type MultisigBackup } from './api';
    import { l } from './i18n';
    import Info from './Info.svelte';
    let {
        snapshotView,
        busy,
        run,
    }: {
        snapshotView: Snapshot;
        busy: boolean;
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
    } = $props();
    let network = $state<Network>('bitcoin');
    let identityId = $state('');
    let account = $state(0);
    let keyPassword = $state('');
    let cosigner = $state('');
    let label = $state('');
    let threshold = $state(2);
    let cosigners = $state('');
    let branch = $state(0);
    let index = $state(0);
    let preview = $state<MultisigBackup | null>(null);
    let confirmed = $state(false);
    const networks: Network[] = ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'];
    const generate = () =>
        run({
            action: async () => {
                try {
                    cosigner = await call<string>({
                        command: 'multisig_cosigner',
                        args: { identityId, network, account, keyPassword: keyPassword || null },
                    });
                } finally {
                    keyPassword = '';
                }
            },
        });
    const inspect = () =>
        run({
            action: async () => {
                preview = await call<MultisigBackup>({
                    command: 'preview_multisig',
                    args: { label, network, threshold, cosigners, branch, index },
                });
                confirmed = false;
            },
        });
    const restore = () =>
        run({
            action: async () => {
                const path = await open({ multiple: false, filters: [{ name: l.multiConfig, extensions: ['avmulti'] }] });
                if (typeof path !== 'string') return;
                preview = await call<MultisigBackup>({ command: 'read_multisig', args: { path } });
                label = preview.label;
                network = preview.network;
                threshold = preview.policy.threshold;
                branch = preview.policy.branch;
                index = preview.policy.index;
                cosigners = preview.policy.cosigners
                    .map((c) => `[${c.fingerprint}/${c.path.slice(2).replaceAll("'", 'h')}]${c.xpub}`)
                    .join('\n');
                confirmed = false;
            },
        });
    const register = () =>
        run({
            action: async () => {
                try {
                    await call({
                        command: 'register_multisig',
                        args: { backup: preview, identityId: identityId || null, keyPassword: keyPassword || null, confirmed },
                    });
                    preview = null;
                    confirmed = false;
                } finally {
                    keyPassword = '';
                }
            },
            success: l.multiRegistered,
        });
    const backup = ({ id }: { id: string }) =>
        run({
            action: async () => {
                const path = await save({
                    defaultPath: `airvault-${id}.avmulti`,
                    filters: [{ name: l.multiConfig, extensions: ['avmulti'] }],
                });
                if (path) await call({ command: 'export_multisig', args: { id, path } });
            },
            success: l.exportDone,
        });
    const nextAddress = ({ id }: { id: string }) => {
        const existing = snapshotView.accounts.find((a) => a.id === id);
        if (!existing?.multisig) return;
        label = existing.label;
        network = existing.network;
        identityId = existing.identityId ?? '';
        threshold = existing.multisig.threshold;
        branch = existing.multisig.branch;
        index = existing.multisig.index + 1;
        cosigners = existing.multisig.cosigners
            .map((c) => `[${c.fingerprint}/${c.path.slice(2).replaceAll("'", 'h')}]${c.xpub}`)
            .join('\n');
        preview = null;
        confirmed = false;
        document.getElementById('multisig-setup')?.scrollIntoView({ block: 'start' });
    };
</script>

<section>
    <div class="heading-with-help">
        <h2>{l.multiYourKey}</h2>
        <Info title={l.multiYourKey} topic="multisig"><p>{l.multiYourKeyHelp}</p></Info>
    </div>
    <div class="multisig-fields">
        <label
            >{l.network}<select
                bind:value={network}
                onchange={() => {
                    cosigner = '';
                    preview = null;
                }}
                >{#each networks as n}<option value={n}>{l['network_' + n] ?? n}</option>{/each}</select
            ></label
        >
        <label
            >{l.identity}<select bind:value={identityId} onchange={() => (cosigner = '')}
                ><option value="">{l.multiWatchOnly}</option
                >{#each snapshotView.identities.filter( (i) => snapshotView.roots.some((r) => r.id === i.rootId && r.kind === 'bip39') ) as identity}<option
                        value={identity.id}
                        >{snapshotView.roots.find((r) => r.id === identity.rootId)?.label} / {identity.label}</option
                    >{/each}</select
            ></label
        >
    </div>
    {#if identityId && !preview}
        <label>{l.multiAccountIndex}<input type="number" min="0" max="2147483647" step="1" bind:value={account} /></label>
        <label>{l.rootPasswordUsed}<input type="password" bind:value={keyPassword} autocomplete="off" /></label>
        <button disabled={busy} onclick={() => void generate()}>{l.multiGetKey}</button>
    {/if}
    {#if cosigner}<label>{l.multiPublicKey}<textarea rows="4" readonly value={cosigner}></textarea></label>
        <button
            disabled={busy}
            onclick={() => void run({ action: () => navigator.clipboard.writeText(cosigner), success: l.copied })}
            >{l.copy}</button
        >
    {/if}
</section>

<section id="multisig-setup">
    <div class="section-heading">
        <div class="heading-with-help">
            <h2>{l.multiSetup}</h2>
            <Info title={l.multiSetup} topic="multisig"><p>{l.multiSetupHelp}</p></Info>
        </div>
        <button disabled={busy} onclick={() => void restore()}>{l.multiLoad}</button>
    </div>
    {#if !preview}
        <label>{l.walletLabel}<input bind:value={label} /></label>
        <div class="multisig-fields">
            <label>{l.multiThreshold}<input type="number" min="1" max="15" step="1" bind:value={threshold} /></label>
            <label
                >{l.multiBranch}<select bind:value={branch}
                    ><option value={0}>{l.multiReceive}</option><option value={1}>{l.multiChange}</option></select
                ></label
            >
            <label>{l.multiAddressIndex}<input type="number" min="0" max="2147483647" step="1" bind:value={index} /></label>
        </div>
        <label>{l.multiCosigners}<textarea rows="6" bind:value={cosigners} spellcheck="false"></textarea></label>
        <p class="muted">{l.multiCosignersHint}</p>
        <button class="primary" disabled={busy || !label || !cosigners} onclick={() => void inspect()}>{l.multiReview}</button>
    {:else}
        <h3>{preview.label} · {preview.policy.threshold} / {preview.policy.cosigners.length} · {preview.network}</h3>
        <p>{l.multiCompare}</p>
        <code class="address">{preview.address}</code>
        <p>{l.multiAddressIndex}: {preview.policy.branch} / {preview.policy.index}</p>
        {#each preview.policy.cosigners as c, i}<details>
                <summary>{l.signer} {i + 1} · {c.fingerprint}</summary>
                <p>{c.path}</p>
                <code class="wrap">{c.xpub}</code>
            </details>{/each}
        <details>
            <summary>{l.multiDescriptors}</summary><code class="wrap">{preview.receiveDescriptor}</code>
            <hr />
            <code class="wrap">{preview.changeDescriptor}</code>
        </details>
        <div class="heading-with-help">
            <h3>{l.multiBackupTitle}</h3>
            <Info title={l.multiBackupTitle} topic="multisigBackup"><p>{l.multiBackupHint}</p></Info>
        </div>
        <p>{l.multiBackupHint}</p>
        {#if identityId}<label>{l.rootPasswordUsed}<input type="password" bind:value={keyPassword} autocomplete="off" /></label
            >{/if}
        <label class="check"><input type="checkbox" bind:checked={confirmed} />{l.multiConfirm}</label>
        <div class="actions">
            <button class="primary" disabled={busy || !confirmed} onclick={() => void register()}>{l.multiRegister}</button
            ><button
                disabled={busy}
                onclick={() => {
                    preview = null;
                    confirmed = false;
                }}>{l.multiEdit}</button
            >
        </div>
    {/if}
</section>

{#each snapshotView.accounts.filter((a) => a.multisig) as a}
    <section>
        <div class="section-heading">
            <h2>{a.label}</h2>
            <span class="badge"
                >{a.multisig!.threshold} / {a.multisig!.cosigners.length} · {a.watchOnly ? l.multiWatchOnly : l.signer}</span
            >
        </div>
        <code class="address">{a.address}</code>
        <p>{a.network} · {l.multiAddressIndex}: {a.multisig!.branch} / {a.multisig!.index}</p>
        <div class="actions">
            <button disabled={busy} onclick={() => void backup({ id: a.id })}>{l.multiSave}</button><button
                disabled={busy}
                onclick={() => nextAddress({ id: a.id })}>{l.multiNext}</button
            >
        </div>
    </section>
{/each}

<style>
    .multisig-fields {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
        gap: 1rem;
    }
</style>
