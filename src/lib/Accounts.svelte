<script lang="ts">
    import { call, type Snapshot, type Network, type Capability, type Account } from './api';
    import { l } from './i18n';
    import Info from './Info.svelte';
    let {
        snapshotView,
        busy,
        run,
        onexport,
        registry,
    }: {
        snapshotView: Snapshot;
        busy: boolean;
        registry: Capability[];
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
        onexport: () => void;
    } = $props();
    let identityId = $state(''),
        label = $state(''),
        path = $state("m/84'/0'/0'/0/0"),
        keyPassword = $state(''),
        search = $state('');
    let network = $state<Network>('bitcoin');
    let tokenEnabled = $state(false),
        tokenContract = $state(''),
        tokenSymbol = $state(''),
        tokenDecimals = $state(18);
    let summary = $state<{
        address: string;
        balance: string;
        symbol: string;
        assets: { contract: string; symbol: string; balance: string }[];
    } | null>(null);
    let tokenAccountId = $state(''),
        existingContract = $state(''),
        existingSymbol = $state(''),
        existingDecimals = $state(18);
    let deleting = $state<Account | null>(null);
    let deleteDialog: HTMLDialogElement;
    const deleteAccount = () =>
        run({
            action: async () => {
                if (!deleting) return;
                await call({ command: 'delete_account', args: { accountId: deleting.id, confirmed: true } });
                if (tokenAccountId === deleting.id) tokenAccountId = '';
                if (summary?.address === deleting.address) summary = null;
                deleting = null;
                deleteDialog.close();
            },
            success: l.accountDeleted,
        });
    const tokenAccount = $derived(snapshotView.accounts.find((a) => a.id === tokenAccountId));
    const tokenChain = $derived(registry.find((c) => c.network === tokenAccount?.network));
    const addToken = () =>
        run({
            action: async () => {
                if (!tokenChain?.tokens) return;
                await call({
                    command: 'add_account_token',
                    args: {
                        accountId: tokenAccountId,
                        token: {
                            standard: tokenChain.tokens.toLowerCase().replace('-', ''),
                            contract: existingContract,
                            symbol: existingSymbol,
                            decimals: existingDecimals,
                        },
                    },
                });
                tokenAccountId = '';
                existingContract = '';
                existingSymbol = '';
            },
            success: l.saved,
        });
    const chain = $derived(registry.find((c) => c.network === network));
    const derive = () =>
        run({
            action: async () => {
                try {
                    await call({
                        command: 'derive_account',
                        args: {
                            identityId,
                            label,
                            network,
                            path,
                            keyPassword: keyPassword || null,
                            token:
                                tokenEnabled && chain?.tokens
                                    ? {
                                          standard: chain.tokens.toLowerCase().replace('-', ''),
                                          contract: tokenContract,
                                          symbol: tokenSymbol,
                                          decimals: tokenDecimals,
                                      }
                                    : null,
                        },
                    });
                } finally {
                    keyPassword = '';
                }
            },
            success: l.saved,
        });
    const copy = ({ text }: { text: string }) => run({ action: () => navigator.clipboard.writeText(text), success: l.copied });
    const balance = ({ accountId }: { accountId: string }) =>
        run({
            action: async () => {
                summary = await call({ command: 'address_summary', args: { accountId } });
            },
        });
</script>

<div class="actions">
    <button onclick={onexport} disabled={busy || !snapshotView.accounts.length}>{l.exportPublic}</button><Info
        title={l.exportPublic}
        topic="exchange"><p>{l.exportPrivacy}</p></Info
    >
</div>
{#if snapshotView.settings?.mode !== 'proxy'}
    <section>
        <h2>{l.derive}</h2>
        <div class="three-columns">
            <label
                >{l.identity}<select bind:value={identityId}
                    ><option value="">{l.select}</option>{#each snapshotView.identities as identity}<option value={identity.id}
                            >{snapshotView.roots.find((r) => r.id === identity.rootId)?.label} / {identity.label}</option
                        >{/each}</select
                ></label
            >
            <label>{l.name}<input bind:value={label} maxlength="120" /></label>
            <label
                >{l.network}<select
                    bind:value={network}
                    onchange={(event) => {
                        network = event.currentTarget.value as Network;
                        path = registry.find((c) => c.network === network)?.path ?? '';
                        tokenEnabled = false;
                        tokenContract = '';
                        tokenSymbol = '';
                        tokenDecimals = network.startsWith('solana') ? 9 : 18;
                    }}
                >
                    {#each registry as item}<option value={item.network}>{item.name}</option>{/each}
                </select></label
            >
        </div>
        <div class="two-columns">
            <div class="field">
                <div class="heading-with-help">
                    <label for="account-path">{l.path}</label><Info title={l.path} topic="paths"><p>{l.pathHelp}</p></Info>
                </div>
                <input id="account-path" bind:value={path} maxlength="120" />
            </div>
            <label>{l.reauth}<input type="password" bind:value={keyPassword} autocomplete="off" /></label>
        </div>
        {#if chain?.tokens}
            <label class="check"><input type="checkbox" bind:checked={tokenEnabled} />{l.addToken} · {chain.tokens}</label>
            {#if tokenEnabled}<div class="three-columns">
                    <label>{l.tokenContract}<input bind:value={tokenContract} spellcheck="false" /></label>
                    <label>{l.tokenSymbol}<input bind:value={tokenSymbol} maxlength="16" placeholder="USDC, …" /></label>
                    <label
                        >{l.tokenDecimals}<input
                            type="number"
                            bind:value={tokenDecimals}
                            min="0"
                            max={network.startsWith('solana') ? 18 : 36}
                        /></label
                    >
                </div>{/if}
        {/if}
        <div class="actions">
            <button
                class="primary"
                disabled={busy || !identityId || !label || (tokenEnabled && (!tokenContract || !tokenSymbol))}
                onclick={() => void derive()}>{l.derive}</button
            >
        </div>
    </section>
{/if}
<label>{l.search}<input bind:value={search} type="search" /></label>
{#if !snapshotView.accounts.length}<section class="empty">{l.emptyAccounts}</section>{/if}
{#each snapshotView.accounts.filter((a) => (a.label + a.address + a.path + a.network)
        .toLowerCase()
        .includes(search.toLowerCase())) as account}
    <section>
        <div class="section-heading">
            <h2>{account.label}</h2>
            <span class="badge"
                >{registry.find((c) => c.network === account.network)?.name ?? account.network} · {account.watchOnly
                    ? l.watchOnly
                    : l.localKey}</span
            >
        </div>
        <code class="address">{account.address}</code>
        <div class="actions">
            <button onclick={() => void copy({ text: account.address })}>{l.copy}</button>
            <button
                class="danger"
                disabled={busy}
                onclick={() => {
                    deleting = account;
                    deleteDialog.showModal();
                }}>{l.deleteAccount}</button
            >
            {#if snapshotView.settings?.mode !== 'offline'}<button
                    disabled={busy}
                    onclick={() => void balance({ accountId: account.id })}>{l.balance}</button
                >{/if}
            {#if registry.find((c) => c.network === account.network)?.tokens}<button
                    disabled={busy}
                    onclick={() => {
                        tokenAccountId = account.id;
                        existingContract = '';
                        existingSymbol = '';
                        existingDecimals = account.network.startsWith('solana') ? 9 : 18;
                    }}>{l.addToken}</button
                >{/if}
            {#each account.tokens ?? [] as token}<span class="badge neutral">{token.symbol} · {token.standard.toUpperCase()}</span
                >{/each}
        </div>
        {#if tokenAccountId === account.id}
            <div class="three-columns">
                <label>{l.tokenContract}<input bind:value={existingContract} spellcheck="false" /></label>
                <label>{l.tokenSymbol}<input bind:value={existingSymbol} maxlength="16" /></label>
                <label
                    >{l.tokenDecimals}<input
                        type="number"
                        min="0"
                        max={account.network.startsWith('solana') ? 18 : 36}
                        bind:value={existingDecimals}
                    /></label
                >
            </div>
            <div class="actions">
                <button disabled={busy || !existingContract || !existingSymbol} onclick={() => void addToken()}>{l.save}</button
                ><button
                    onclick={() => {
                        tokenAccountId = '';
                    }}>{l.cancel}</button
                >
            </div>
        {/if}
        <details>
            <summary>{l.details}</summary>
            <dl>
                <dt>{l.path}</dt>
                <dd><code>{account.path || l.rootKey}</code></dd>
                <dt>{l.publicKey}</dt>
                <dd><code class="wrap">{account.publicKey}</code></dd>
                <dt>{l.identity}</dt>
                <dd>{snapshotView.identities.find((i) => i.id === account.identityId)?.label ?? l.watchOnly}</dd>
                {#each account.tokens ?? [] as token}<dt>{token.symbol}</dt>
                    <dd><code>{token.contract}</code> · {token.decimals} {l.tokenDecimals.toLowerCase()}</dd>{/each}
            </dl>
        </details>
    </section>
{/each}
{#if summary}<section>
        <div class="section-heading">
            <div class="heading-with-help">
                <h2>{l.balanceReported}</h2>
                <Info title={l.balanceReported} topic="providers"><p>{l.externalHelp}</p></Info>
            </div>
        </div>
        <code class="address">{summary.address}</code>
        <p class="balance-value">{summary.balance} {summary.symbol}</p>
        {#each summary.assets as token}<p>{token.balance} {token.symbol} <code class="wrap">{token.contract}</code></p>{/each}
    </section>{/if}

<dialog
    bind:this={deleteDialog}
    aria-labelledby="delete-account-title"
    class="info-dialog"
    oncancel={() => {
        deleting = null;
    }}
>
    <h2 id="delete-account-title">{l.deleteAccountConfirm}</h2>
    {#if deleting}
        <strong>{deleting.label}</strong>
        <code class="address">{deleting.address}</code>
        <dl>
            <dt>{l.network}</dt>
            <dd>{deleting.network}</dd>
            <dt>{l.identity}</dt>
            <dd>{snapshotView.identities.find((identity) => identity.id === deleting?.identityId)?.label ?? l.watchOnly}</dd>
            <dt>{l.path}</dt>
            <dd><code>{deleting.path || l.rootKey}</code></dd>
        </dl>
    {/if}
    <p>{l.deleteAccountHelp}</p>
    <div class="actions">
        <button
            disabled={busy}
            onclick={() => {
                deleting = null;
                deleteDialog.close();
            }}>{l.cancel}</button
        >
        <button
            class="danger"
            disabled={busy || !deleting}
            onclick={() => {
                deleteDialog.close();
                void deleteAccount();
            }}>{l.deleteAccountApprove}</button
        >
    </div>
</dialog>
