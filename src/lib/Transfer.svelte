<script lang="ts">
    import { call, type Snapshot, type Capability, type Network } from './api';
    import { l } from './i18n';
    import Info from './Info.svelte';
    import BitcoinTransfer from './BitcoinTransfer.svelte';
    let {
        snapshotView,
        busy,
        run,
        onprepared,
        registry,
    }: {
        snapshotView: Snapshot;
        busy: boolean;
        registry: Capability[];
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
        onprepared: () => void;
    } = $props();
    const bitcoins = ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'];
    const evms = ['ethereum', 'sepolia', 'bnb', 'polygon', 'avalanche', 'arbitrum', 'optimism', 'base'];
    let accountId = $state(''),
        recipient = $state(''),
        amount = $state(''),
        assetIndex = $state(-1);
    let parameters = $state<Record<string, string>>({});
    let cosmosAction = $state('send');
    let nft = $state(''),
        collection = $state(''),
        tokenId = $state('');
    const source = $derived(snapshotView.accounts.find((a) => a.id === accountId));
    const chain = $derived(registry.find((c) => c.network === source?.network));
    const asset = $derived(source?.tokens?.[assetIndex]);
    const kind = $derived(
        source
            ? evms.includes(source.network)
                ? 'evm'
                : source.network.startsWith('solana')
                  ? 'solana'
                  : source.network.startsWith('cardano')
                    ? 'cardano'
                    : source.network.startsWith('tron')
                      ? 'tron'
                      : ['dogecoin', 'litecoin', 'bitcoin_cash'].includes(source.network)
                        ? 'utxo'
                        : source.network
            : '',
    );
    const definitions: Record<string, [string, string, string][]> = {
        evm: [
            ['nonce', 'nonce', '0'],
            ['gasLimit', 'gasLimit', '21000'],
            ['gasPrice', 'gasPrice', '1000000000'],
        ],
        solana: [['recentBlockhash', 'recentBlockhash', '']],
        cosmos: [
            ['accountNumber', 'accountNumber', '0'],
            ['sequence', 'sequence', '0'],
            ['gasLimit', 'gasLimit', '200000'],
            ['fee', 'baseFee', '5000'],
            ['memo', 'memo', ''],
        ],
        xrp: [
            ['sequence', 'sequence', '0'],
            ['lastLedgerSequence', 'lastLedgerSequence', ''],
            ['fee', 'baseFee', '12'],
            ['destinationTag', 'destinationTag', ''],
        ],
        tron: [
            ['refBlockBytes', 'refBlockBytes', ''],
            ['refBlockHash', 'refBlockHash', ''],
            ['timestamp', 'timestamp', ''],
            ['expiration', 'expiration', ''],
            ['feeLimit', 'feeLimit', '0'],
        ],
        utxo: [
            ['fee', 'baseFee', '1000'],
            ['utxos', 'utxoJson', '[]'],
        ],
        cardano: [
            ['fee', 'baseFee', '200000'],
            ['invalidAfter', 'invalidAfter', ''],
            ['utxos', 'utxoJson', '[]'],
        ],
        polkadot: [
            ['nonce', 'nonce', '0'],
            ['tip', 'tip', '0'],
            ['specVersion', 'specVersion', ''],
            ['transactionVersion', 'txVersion', ''],
            ['specName', 'runtimeName', 'statemint'],
            ['metadata', 'runtimeMetadata', ''],
        ],
    };
    const reset = () => {
        const a = snapshotView.accounts.find((item) => item.id === accountId);
        const network = a?.network ?? '';
        const family = evms.includes(network)
            ? 'evm'
            : network.startsWith('solana')
              ? 'solana'
              : network.startsWith('cardano')
                ? 'cardano'
                : network.startsWith('tron')
                  ? 'tron'
                  : ['dogecoin', 'litecoin', 'bitcoin_cash'].includes(network)
                    ? 'utxo'
                    : network;
        parameters = Object.fromEntries((definitions[family] ?? []).map(([key, , value]) => [key, value]));
        cosmosAction = 'send';
        recipient = '';
        amount = '';
        assetIndex = -1;
        nft = '';
        collection = '';
        tokenId = '';
    };
    const loadNetwork = () =>
        run({
            action: async () => {
                const values = await call<Record<string, unknown>>({ command: 'fetch_chain_state', args: { accountId } });
                for (const [key, value] of Object.entries(values))
                    parameters[key] = typeof value === 'object' ? JSON.stringify(value, null, 2) : String(value);
            },
        });
    const integer = ({ key }: { key: string }) => {
        if (!/^\d+$/.test(parameters[key] ?? '')) throw new Error(l[key] + ': ' + l.wholeNumber);
        const value = Number(parameters[key]);
        if (!Number.isSafeInteger(value)) throw new Error(l.wholeNumber);
        return value;
    };
    const baseUnits = ({ value, decimals }: { value: string; decimals: number }) => {
        if (!/^(0|[1-9]\d*)(\.\d+)?$/.test(value)) throw new Error(l.invalidAmount);
        const [whole, fraction = ''] = value.split('.');
        if (fraction.length > decimals) throw new Error(l.invalidAmount);
        return BigInt(whole + fraction.padEnd(decimals, '0')).toString();
    };
    const prepare = () =>
        run({
            action: async () => {
                if (!source || !chain) throw new Error(l.source);
                const operation: Record<string, unknown> = {
                    type: kind,
                    to: recipient,
                    amount: baseUnits({
                        value: kind === 'cosmos' && cosmosAction === 'claim_rewards' ? '0' : amount,
                        decimals: nft ? 0 : (asset?.decimals ?? chain.decimals),
                    }),
                };
                const numeric = [
                    'nonce',
                    'gasLimit',
                    'sequence',
                    'accountNumber',
                    'lastLedgerSequence',
                    'timestamp',
                    'expiration',
                    'feeLimit',
                    'invalidAfter',
                    'specVersion',
                    'transactionVersion',
                ];
                for (const [key] of definitions[kind] ?? []) {
                    operation[key] =
                        key === 'utxos'
                            ? JSON.parse(parameters[key])
                            : key === 'destinationTag'
                              ? parameters[key] === ''
                                  ? null
                                  : integer({ key })
                              : numeric.includes(key)
                                ? integer({ key })
                                : parameters[key];
                }
                if (kind === 'cosmos') operation.action = cosmosAction;
                if (asset) {
                    operation.asset =
                        kind === 'solana'
                            ? { mint: asset.contract, symbol: asset.symbol, decimals: asset.decimals }
                            : kind === 'tron'
                              ? { contract: asset.contract, symbol: asset.symbol, decimals: asset.decimals }
                              : { standard: 'erc20', contract: asset.contract, symbol: asset.symbol, decimals: asset.decimals };
                }
                if (nft) operation.asset = { standard: nft, contract: collection, token_id: tokenId };
                await call({ command: 'prepare_chain_transfer', args: { accountId, operation } });
                onprepared();
            },
            success: l.savedRequest,
        });
</script>

<section>
    <label
        >{l.source}<select bind:value={accountId} onchange={reset}
            ><option value="">{l.select}</option>{#each snapshotView.accounts as account}<option value={account.id}
                    >{account.label} · {registry.find((c) => c.network === account.network)?.name ?? account.network}</option
                >{/each}</select
        ></label
    >
</section>
{#if source && bitcoins.includes(source.network)}
    {#key accountId}<BitcoinTransfer {snapshotView} {busy} {run} {onprepared} initialAccountId={accountId} />{/key}
{:else if source && chain}
    <section>
        <div class="section-heading">
            <div class="heading-with-help">
                <h2>{chain.name}</h2>
                <Info title={l.transfer} topic="online"><p>{l.transferHelp}</p></Info>
            </div>
        </div>
        {#if source.tokens?.length}<label
                >{l.asset}<select
                    bind:value={assetIndex}
                    onchange={() => {
                        nft = '';
                        amount = '';
                        if (kind === 'evm') parameters.gasLimit = assetIndex < 0 ? '21000' : '100000';
                        if (kind === 'tron') parameters.feeLimit = assetIndex < 0 ? '0' : '100000000';
                    }}
                    ><option value={-1}>{chain.symbol}</option>{#each source.tokens as token, index}<option value={index}
                            >{token.symbol} · {token.contract}</option
                        >{/each}</select
                ></label
            >{/if}
        {#if kind === 'evm'}
            <label
                >{l.nftTransfer}<select
                    bind:value={nft}
                    onchange={() => {
                        assetIndex = -1;
                        if (nft) {
                            parameters.gasLimit = '150000';
                            amount = '1';
                        }
                    }}
                    ><option value="">{l.none}</option><option value="erc721">ERC-721</option><option value="erc1155"
                        >ERC-1155</option
                    ></select
                ></label
            >
            {#if nft}<div class="two-columns">
                    <label>{l.tokenContract}<input bind:value={collection} /></label><label
                        >{l.tokenId}<input bind:value={tokenId} /></label
                    >
                </div>{/if}
        {/if}
        {#if kind === 'cosmos'}<label
                >{l.stakingAction}<select bind:value={cosmosAction}
                    ><option value="send">{l.cosmosSend}</option><option value="delegate">{l.cosmosDelegate}</option><option
                        value="undelegate">{l.cosmosUndelegate}</option
                    ><option value="claim_rewards">{l.cosmosClaim}</option></select
                ></label
            >{/if}
        <label
            >{kind === 'cosmos' && cosmosAction !== 'send' ? l.validator : l.recipient}<input
                bind:value={recipient}
                spellcheck="false"
            /></label
        >
        {#if kind !== 'cosmos' || cosmosAction !== 'claim_rewards'}<label
                >{l.sendAmount} · {nft ? l.nftUnits : (asset?.symbol ?? chain.symbol)}<input
                    bind:value={amount}
                    inputmode="decimal"
                /></label
            >{/if}
        <div class="section-heading">
            <h3>{l.transactionDetails}</h3>
            <button disabled={busy || !snapshotView.settings?.providers?.[source.network]} onclick={() => void loadNetwork()}
                >{l.loadNetwork}</button
            >
        </div>
        <div class="two-columns">
            {#each definitions[kind] ?? [] as [key, label]}
                <label class:full-width={key === 'utxos' || key === 'metadata'}
                    >{l[label]}
                    {#if key === 'utxos' || key === 'metadata'}<textarea
                            bind:value={parameters[key]}
                            spellcheck="false"
                            class="code-input"></textarea>
                    {:else}<input bind:value={parameters[key]} spellcheck="false" />{/if}
                </label>
            {/each}
        </div>
        <button
            class="primary"
            disabled={busy || !recipient || (!amount && !(kind === 'cosmos' && cosmosAction === 'claim_rewards'))}
            onclick={() => void prepare()}>{l.prepare}</button
        >
    </section>
{/if}
