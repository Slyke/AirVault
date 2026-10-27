<script lang="ts">
    import { onMount } from 'svelte';
    import { call, type Snapshot, type Review } from './api';
    import Info from './Info.svelte';
    import PayloadInspector from './PayloadInspector.svelte';
    let showInspector = $state(false);
    import { l } from './i18n';
    let {
        snapshotView,
        review,
        busy,
        run,
        onclose,
        error = '',
    }: {
        snapshotView: Snapshot;
        review: Review;
        busy: boolean;
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
        onclose: () => void;
        error?: string;
    } = $props();
    let dialog: HTMLDialogElement;
    let approved = $state(false),
        broadcastApproved = $state(false),
        password = $state('');
    let passwords = $state<Record<string, string>>({}),
        selected = $state<Record<string, string>>({});
    const response = $derived(snapshotView.responses.find((r) => r.requestId === review.inspection.requestId));
    const hasUnsignedLocal = $derived(
        review.inspection.inputs.some((input, index) =>
            snapshotView.accounts.some(
                (account) =>
                    input.accountIds.includes(account.id) &&
                    !account.watchOnly &&
                    account.identityId &&
                    !review.inspection.multisig?.[index]?.fingerprints.includes(account.fingerprint),
            ),
        ),
    );
    const sign = () =>
        run({
            action: async () => {
                try {
                    await call({
                        command: review.inspection.multisig ? 'approve_multisig' : 'approve_signing',
                        args: {
                            token: review.token,
                            confirmed: approved,
                            password,
                            keyPasswords: passwords,
                            selectedAccounts: Object.values(selected),
                        },
                    });
                    onclose();
                } finally {
                    password = '';
                    passwords = {};
                    approved = false;
                }
            },
            success: review.inspection.multisig ? l.multiSigned : l.signed,
        });
    const broadcast = () =>
        run({
            action: async () => {
                try {
                    await call({
                        command: 'broadcast_response',
                        args: { id: review.inspection.requestId, password, confirmed: broadcastApproved },
                    });
                    onclose();
                } finally {
                    password = '';
                    broadcastApproved = false;
                }
            },
            success: l.broadcasted,
        });
    onMount(() => {
        dialog.showModal();
    });
</script>

<dialog
    bind:this={dialog!}
    oncancel={(event) => {
        event.preventDefault();
        if (!busy) onclose();
    }}
>
    <div class="dialog-heading">
        <div>
            <div class="eyebrow">{l.review}</div>
            <div class="heading-with-help">
                <h2>{review.inspection.chain?.title ?? l.native} / {review.inspection.network}</h2>
                <Info title={l.review} topic="signing"
                    ><p>{l.reviewHelp}</p>
                    <p>{l.externalHelp}</p></Info
                >
            </div>
        </div>
        <button onclick={onclose} disabled={busy}>{l.close}</button>
    </div>

    {#if error}<div class="alert danger" role="alert">{error}</div>{/if}

    {#if review.inspection.chain}
        <div class="review-output">
            <strong>{review.inspection.chain.amount}</strong>
            <p>{l.networkFee}: {review.inspection.chain.fee}</p>
        </div>
        <dl class="chain-details">
            {#each review.inspection.chain.fields as [key, value]}<dt>{key.replace(/([a-z])([A-Z])/g, '$1 $2')}</dt>
                <dd><code>{value}</code></dd>{/each}
        </dl>
    {:else}
        <h3>{l.outputs}</h3>
        {#each review.inspection.outputs as output, index}<article class="review-output">
                <div class="section-heading">
                    <span>{index + 1}. {output.knownAccount ? l.known : l.destination}</span><strong
                        >{output.satoshis.toLocaleString()} {l.sats}</strong
                    >
                </div>
                <code class="address">{output.address}</code>
            </article>{/each}
        <div class="stats compact">
            <article><small>{l.totalInput}</small><strong>{review.inspection.totalInputSats.toLocaleString()}</strong></article>
            <article><small>{l.networkFee}</small><strong>{review.inspection.feeSats.toLocaleString()}</strong></article>
            <article><small>{l.feeRate}</small><strong>{review.inspection.feeRate.toFixed(2)}</strong></article>
        </div>
    {/if}
    <details
        ontoggle={(event) => {
            showInspector = event.currentTarget.open;
        }}
    >
        <summary>{l.inspectorInspect}</summary>
        {#if showInspector}<PayloadInspector
                requestId={review.inspection.requestId}
                bitcoin={!review.inspection.chain}
                hasResponse={!!response}
            />{/if}
    </details>
    <h3>{l.inputs}</h3>
    {#each review.inspection.inputs as input, inputIndex}
        {@const matches = snapshotView.accounts.filter((a) => input.accountIds.includes(a.id) && !a.watchOnly)}
        <article class="review-input">
            {#if review.inspection.multisig}
                {@const status = review.inspection.multisig[inputIndex]}
                <div class="heading-with-help">
                    <h3>{status.signed} / {status.required} {l.multiSignatures}</h3>
                    <Info title={l.multisig} topic="multisigSigning"><p>{l.multiProgressHelp}</p></Info>
                </div>
                <p>{l.multiCosignerCount}: {status.total}</p>
                {#if status.fingerprints.length}<p>{l.multiSignedBy}: {status.fingerprints.join(', ')}</p>{/if}
                <details>
                    <summary>{l.multiConfig}</summary
                    >{#each snapshotView.accounts.find((a) => input.accountIds.includes(a.id) && a.multisig)?.multisig?.cosigners ?? [] as cosigner}<p
                        >
                            {cosigner.fingerprint} · {cosigner.path}
                        </p>
                        <code class="wrap">{cosigner.xpub}</code>{/each}
                </details>
            {/if}
            <code class="address">{input.address}</code>{#if !review.inspection.chain}<strong
                    >{input.satoshis.toLocaleString()} {l.sats}</strong
                >{/if}
            {#if matches.length === 1}<p>{l.signer}: {matches[0].label} · {matches[0].path}</p>
            {:else if matches.length > 1}<label
                    >{l.selectSigner}<select bind:value={selected[input.outpoint]}
                        ><option value="">{l.select}</option>{#each matches as account}<option value={account.id}
                                >{account.label} · {account.path}</option
                            >{/each}</select
                    ></label
                >
            {:else}<p class="muted">{l.noSigner}</p>{/if}
            {#if !review.inspection.chain}<details>
                    <summary>{l.advanced}</summary>
                    <dl>
                        <dt>{l.outpoint}</dt>
                        <dd><code class="wrap">{input.outpoint}</code></dd>
                        <dt>{l.sequence}</dt>
                        <dd>{input.sequence}</dd>
                        <dt>{l.sighash}</dt>
                        <dd>{input.sighash}</dd>
                    </dl>
                </details>{/if}
        </article>
    {/each}
    <details>
        <summary>{l.advanced}</summary>
        <dl>
            <dt>{l.digest}</dt>
            <dd><code class="wrap">{review.inspection.digest}</code></dd>
            <dt>{l.txVersion}</dt>
            <dd>{review.inspection.version}</dd>
            <dt>{l.lockTime}</dt>
            <dd>{review.inspection.lockTime}</dd>
        </dl>
        <label
            >{l.rawRequest}<textarea
                readonly
                value={snapshotView.pending.find((r) => r.requestId === review.inspection.requestId)?.payload.data}
            ></textarea></label
        >
    </details>
    {#if response}<div class="alert mid">{l.response}<code class="wrap">{response.txid}</code></div>{/if}
    {#if !response && review.inspection.multisig && !hasUnsignedLocal}<p class="alert mid">{l.multiWaiting}</p>{/if}
    {#if !response && (!review.inspection.multisig || hasUnsignedLocal) && snapshotView.settings?.mode !== 'proxy'}
        <div class="heading-with-help">
            <h3>{l.signing}</h3>
            <Info title={l.signing} topic={review.inspection.multisig ? 'multisigSigning' : 'signing'}><p>{l.signHelp}</p></Info>
        </div>
        <label>{l.password}<input type="password" bind:value={password} autocomplete="off" /></label>
        {#each snapshotView.roots.filter((r) => r.customPassword && review.inspection.inputs.some( (i) => snapshotView.accounts.some((a) => i.accountIds.includes(a.id) && snapshotView.identities.some((id) => id.id === a.identityId && id.rootId === r.id)) )) as root}<label
                >{root.label} / {l.reauth}<input type="password" bind:value={passwords[root.id]} autocomplete="off" /></label
            >{/each}
        <label class="check"><input type="checkbox" bind:checked={approved} />{l.approveCheck}</label><button
            class="primary"
            disabled={busy || !approved || !password}
            onclick={() => void sign()}>{l.approve}</button
        >
    {:else if response && snapshotView.settings?.mode !== 'offline'}
        <label>{l.password}<input type="password" bind:value={password} autocomplete="off" /></label><label class="check"
            ><input type="checkbox" bind:checked={broadcastApproved} />{l.broadcastCheck}</label
        ><button
            class="primary"
            disabled={busy ||
                !broadcastApproved ||
                !password ||
                !(snapshotView.settings?.providers?.[review.inspection.network] || snapshotView.settings?.rpcUrl)}
            onclick={() => void broadcast()}>{l.broadcast}</button
        >
    {/if}
</dialog>
