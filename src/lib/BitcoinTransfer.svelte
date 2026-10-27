<script lang="ts">
    import { untrack } from 'svelte';
    import { call, satoshis, type Snapshot } from './api';
    import Info from './Info.svelte';
    import { l } from './i18n';
    let {
        snapshotView,
        initialAccountId,
        busy,
        run,
        onprepared,
    }: {
        snapshotView: Snapshot;
        initialAccountId: string;
        busy: boolean;
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
        onprepared: () => void;
    } = $props();
    let accountId = $state(untrack(() => initialAccountId)),
        recipient = $state(''),
        amount = $state('10000'),
        fee = $state('1000'),
        change = $state(untrack(() => snapshotView.accounts.find((a) => a.id === initialAccountId)?.address ?? '')),
        utxos = $state('[]');
    const fetch = () =>
        run({
            action: async () => {
                utxos = JSON.stringify(await call({ command: 'fetch_utxos', args: { accountId } }), null, 2);
            },
        });
    const prepare = () =>
        run({
            action: async () => {
                const source = snapshotView.accounts.find((a) => a.id === accountId);
                if (!source) throw new Error(l.source);
                await call({
                    command: 'prepare_transfer',
                    args: {
                        network: source.network,
                        utxos: JSON.parse(utxos),
                        recipients: [{ address: recipient, satoshis: satoshis({ value: amount }) }],
                        change,
                        feeSats: satoshis({ value: fee }),
                    },
                });
                onprepared();
            },
            success: l.savedRequest,
        });
</script>

<section>
    <h2>{l.transfer}</h2>
    <label
        >{l.source}<select
            bind:value={accountId}
            onchange={() => {
                change = snapshotView.accounts.find((a) => a.id === accountId)?.address ?? '';
            }}
            ><option value="">{l.select}</option
            >{#each snapshotView.accounts.filter( (a) => ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'].includes(a.network) ) as account}<option
                    value={account.id}>{account.label} · {account.network} · {account.address}</option
                >{/each}</select
        ></label
    ><label>{l.recipient}<input bind:value={recipient} spellcheck="false" /></label>
    <div class="two-columns">
        <label>{l.amount}<input bind:value={amount} inputmode="numeric" /></label><label
            >{l.fee}<input bind:value={fee} inputmode="numeric" /></label
        >
    </div>
    <label
        >{l.change}<select bind:value={change}
            ><option value="">{l.select}</option
            >{#each snapshotView.accounts.filter((a) => a.network === snapshotView.accounts.find((b) => b.id === accountId)?.network) as account}<option
                    value={account.address}>{account.label} · {account.address}</option
                >{/each}</select
        ></label
    >
</section>
<section>
    <div class="section-heading">
        <h2>{l.utxos}</h2>
        <button
            disabled={busy ||
                !accountId ||
                !(
                    snapshotView.settings?.providers?.[snapshotView.accounts.find((a) => a.id === accountId)?.network ?? ''] ||
                    snapshotView.settings?.rpcUrl
                )}
            onclick={() => void fetch()}>{l.fetchUtxos}</button
        >
    </div>

    <div class="field">
        <div class="heading-with-help">
            <label for="transfer-utxos">{l.utxoJson}</label><Info title={l.utxos} topic="fees"><p>{l.utxoHelp}</p></Info>
        </div>
        <textarea id="transfer-utxos" class="code-input" bind:value={utxos} spellcheck="false"></textarea>
    </div>
    <code class="hint">[{'{'}"transactionHex": "...", "vout": 0{'}'}]</code>
    <div class="actions">
        <button class="primary" disabled={busy || !accountId || !recipient} onclick={() => void prepare()}>{l.prepare}</button>
    </div>
</section>
