<script lang="ts">
    import { onMount, setContext } from 'svelte';
    import { helpContext, pageTopics, type HelpNavigation } from './lib/help';
    import { open, save } from '@tauri-apps/plugin-dialog';
    import {
        call,
        emptySnapshot,
        errorText,
        type Snapshot,
        type Mode,
        type Review,
        type Capability,
        type Network,
    } from './lib/api';
    import { l } from './lib/i18n';
    import Wallets from './lib/Wallets.svelte';
    import PayloadInspector from './lib/PayloadInspector.svelte';
    import type { PayloadReport } from './lib/api';
    import Accounts from './lib/Accounts.svelte';
    import Multisig from './lib/Multisig.svelte';
    import Transfer from './lib/Transfer.svelte';
    import RequestReview from './lib/RequestReview.svelte';
    import Help from './lib/Help.svelte';
    import About from './lib/About.svelte';
    import Appearance from './lib/Appearance.svelte';
    import PasswordStrength from './lib/PasswordStrength.svelte';
    import PasswordMatch from './lib/PasswordMatch.svelte';
    import Info from './lib/Info.svelte';
    import { scaleShortcut } from './lib/appearance';
    import SettingsView from './lib/Settings.svelte';
    let snapshotView = $state<Snapshot>(emptySnapshot);
    let page = $state('overview');
    let inspectorInitial = $state<PayloadReport | null>(null);
    let inspectorPath = $state('');
    let inspectorRequest = $state('');
    let helpTopic = $state('start');
    setContext<HelpNavigation>(helpContext, {
        available: () => snapshotView.unlocked,
        open: (topic) => {
            helpTopic = topic;
            review = null;
            page = 'help';
            error = '';
            notice = '';
        },
    });
    let busy = $state(false);
    let error = $state('');
    let notice = $state('');
    let ready = $state(false);
    let showAbout = $state(false);
    let aboutDialog: HTMLDialogElement;
    let password = $state('');
    let passwordConfirm = $state('');
    let mode = $state<Mode>('offline');
    let review = $state<Review | null>(null);
    let registry = $state<Capability[]>([]);
    let dryRun = $state(true);
    let psbtNetwork = $state<Network>('bitcoin');
    let lastActivity = 0;
    let wasUnlocked = false;
    const refresh = async () => {
        snapshotView = await call<Snapshot>({ command: 'snapshot' });
        if (wasUnlocked && !snapshotView.unlocked) {
            review = null;
            password = '';
            passwordConfirm = '';
            page = 'overview';
            inspectorInitial = null;
            inspectorPath = '';
            inspectorRequest = '';
        }
        wasUnlocked = snapshotView.unlocked;
    };
    const run = async ({ action, success = '' }: { action: () => Promise<unknown>; success?: string }) => {
        if (busy) return;
        busy = true;
        error = '';
        notice = '';
        try {
            await action();
            await refresh();
            notice = success;
        } catch (cause) {
            error = errorText({ error: cause });
        } finally {
            busy = false;
        }
    };
    const markActivity = () => {
        if (!snapshotView.unlocked || Date.now() - lastActivity < 10000) return;
        lastActivity = Date.now();
        void call({ command: 'activity' }).catch(() => {
            error = l.locked;
        });
    };
    const authenticate = () =>
        run({
            action: async () => {
                try {
                    if (!snapshotView.initialized && password !== passwordConfirm) throw new Error(l.wrongConfirmation);
                    await call({ command: snapshotView.initialized ? 'unlock_vault' : 'create_vault', args: { password, mode } });
                } finally {
                    password = '';
                    passwordConfirm = '';
                }
            },
        });
    const importFile = () =>
        run({
            action: async () => {
                const path = await open({
                    multiple: false,
                    filters: [{ name: 'AirVault', extensions: ['avpub', 'avreq', 'avsig'] }],
                });
                if (typeof path === 'string') {
                    inspectorInitial = await call<PayloadReport>({ command: 'inspect_file', args: { path, hint: 'auto' } });
                    inspectorPath = path;
                    inspectorRequest = '';
                    page = 'inspector';
                    await call({ command: 'import_file', args: { path } });
                }
            },
            success: l.imported,
        });
    const exportFile = ({ kind, id }: { kind: 'public' | 'request' | 'response' | 'psbt' | 'transaction'; id?: string }) =>
        run({
            action: async () => {
                const ext = { public: 'avpub', request: 'avreq', response: 'avsig', psbt: 'psbt', transaction: 'txn' }[kind];
                const path = await save({
                    defaultPath: `airvault-${id ?? 'accounts'}.${ext}`,
                    filters: [{ name: 'AirVault', extensions: [ext] }],
                });
                if (path) await call({ command: `export_${kind}`, args: { path, id } });
            },
            success: l.exportDone,
        });
    const importPsbt = () =>
        run({
            action: async () => {
                const path = await open({ multiple: false, filters: [{ name: l.multiPsbt, extensions: ['psbt'] }] });
                if (typeof path === 'string') {
                    inspectorInitial = await call<PayloadReport>({ command: 'inspect_file', args: { path, hint: 'auto' } });
                    inspectorPath = path;
                    inspectorRequest = '';
                    page = 'inspector';
                    await call({ command: 'import_psbt', args: { path, network: psbtNetwork } });
                }
            },
            success: l.imported,
        });
    const openReview = ({ id }: { id: string }) =>
        run({
            action: async () => {
                review = await call<Review>({ command: 'review_request', args: { id } });
            },
        });
    const restore = () =>
        run({
            action: async () => {
                try {
                    const path = await open({
                        multiple: false,
                        filters: [{ name: 'Encrypted AirVault backup', extensions: ['avvault'] }],
                    });
                    if (typeof path === 'string') await call({ command: 'restore_backup', args: { path, password, dryRun } });
                } finally {
                    password = '';
                }
            },
            success: dryRun ? l.restoreChecked : l.restoreDone,
        });
    onMount(() => {
        void run({
            action: async () => {
                await refresh();
                registry = await call<Capability[]>({ command: 'capabilities' });
                ready = true;
            },
        });
        const timer = setInterval(() => {
            if (!busy)
                void refresh().catch((cause) => {
                    error = errorText({ error: cause });
                });
        }, 2000);
        return () => clearInterval(timer);
    });
    const navigation = [
        'overview',
        'createWallet',
        'wallets',
        'passphrases',
        'accounts',
        'multisig',
        'transfer',
        'requests',
        'inspector',
        'history',
        'networks',
        'settings',
        'help',
        'about',
    ];
</script>

<svelte:window
    onpointerdown={markActivity}
    onkeydown={(event) => {
        markActivity();
        scaleShortcut({ event });
    }}
/>
{#if !snapshotView.unlocked}
    <main class="gate">
        <div class="gate-brand">
            <span class="logo">AV</span>
            <div>
                <h1>{l.title}</h1>
                <p>{l.subtitle}</p>
            </div>
        </div>
        <details><summary>{l.appearance}</summary><Appearance /></details>
        <div class="gate-card">
            <div class="heading-with-help">
                <h2>{snapshotView.initialized ? l.startup : l.setup}</h2>
                <Info title={l.secureVault}><p>{l.secureVaultHelp}</p></Info>
            </div>
            <p class="muted">{snapshotView.initialized ? l.passwordHelp : l.setupHint}</p>
            {#if error}<div class="alert danger" role="alert">{error}</div>{/if}
            {#if notice}<div class="alert mid" role="status">{notice}</div>{/if}
            <form
                onsubmit={(event) => {
                    event.preventDefault();
                    void authenticate();
                }}
            >
                <label
                    >{l.password}<input
                        type="password"
                        bind:value={password}
                        autocomplete="off"
                        required
                        minlength={snapshotView.initialized ? 1 : 6}
                        maxlength="1024"
                    /></label
                >
                {#if !snapshotView.initialized}<PasswordStrength {password} /><label
                        >{l.passwordConfirm}<input
                            type="password"
                            bind:value={passwordConfirm}
                            autocomplete="off"
                            required
                            minlength="6"
                            maxlength="1024"
                        /></label
                    ><PasswordMatch {password} confirmation={passwordConfirm} />{/if}
                <button class="primary wide" disabled={busy || !ready}
                    >{busy ? l.working : snapshotView.initialized ? l.unlock : l.create}</button
                >
            </form>
            {#if !snapshotView.initialized}<details>
                    <summary>{l.restore}</summary>
                    <div class="heading-with-help">
                        <h3>{l.restoreAction}</h3>
                        <Info title={l.restore}><p>{l.restoreHelp}</p></Info>
                    </div>
                    <label class="check"><input type="checkbox" bind:checked={dryRun} />{l.dryRun}</label><button
                        disabled={busy || !password}
                        onclick={() => void restore()}>{l.restoreAction}</button
                    >
                </details>{/if}
        </div>
        <div class="gate-footer">
            <span>{snapshotView.initialized ? l.encrypted : l.offline}</span><button
                onclick={() => {
                    showAbout = true;
                    aboutDialog.showModal();
                }}>{l.about}</button
            >
        </div>
    </main>
{:else}
    <div class="app-shell">
        <aside>
            <div class="brand">
                <span class="logo">AV</span>
                <div><strong>{l.title}</strong><small>0.1.0</small></div>
            </div>
            <div class="vault-state">
                <span class="status-dot"></span>{l.unlocked}<small>{l[snapshotView.settings?.mode ?? 'offline']}</small>
            </div>
            <nav aria-label={l.tools}>
                {#each navigation as item}{#if !(['createWallet', 'wallets', 'passphrases'].includes(item) && snapshotView.settings?.mode === 'proxy') && !(item === 'transfer' && snapshotView.settings?.mode === 'offline')}
                        <button
                            class:active={page === item}
                            onclick={() => {
                                page = item;
                                error = '';
                                notice = '';
                            }}>{l[item]}</button
                        >
                    {/if}{/each}
            </nav>
            <div class="sidebar-bottom">
                <button onclick={() => void run({ action: () => call({ command: 'lock_vault' }) })}>{l.lock}</button>
            </div>
        </aside>
        <div class="workspace">
            <header>
                <div class="breadcrumb">{l.vault} / <strong>{l[page]}</strong></div>
                <div class="toolbar">
                    <span class:warning={snapshotView.settings?.mode === 'combined'} class="badge"
                        >{l[snapshotView.settings?.mode ?? 'offline']}</span
                    ><button onclick={() => void importFile()} disabled={busy}>{l.import}</button>
                </div>
            </header>
            <main class="content">
                <div class="page-title">
                    <div>
                        <div class="eyebrow">{l.title.toUpperCase()} / {l[page].toUpperCase()}</div>
                        <div class="heading-with-help">
                            <h1>{l[page]}</h1>
                            <Info title={l[page]} topic={page === 'help' ? undefined : pageTopics[page]}
                                ><p>{l['page_' + page]}</p></Info
                            >
                        </div>
                    </div>
                </div>
                {#if error}<div class="alert danger" role="alert">{error}</div>{/if}
                {#if notice}<div class="alert mid" role="status">{notice}</div>{/if}
                {#if busy}<div class="working" role="status">{l.working}</div>{/if}
                {#if page === 'overview'}
                    <div class="stats">
                        <article>
                            <small>{l.walletCount}</small><strong>{snapshotView.roots.length.toString().padStart(2, '0')}</strong>
                        </article>
                        <article>
                            <small>{l.accountCount}</small><strong
                                >{snapshotView.accounts.length.toString().padStart(2, '0')}</strong
                            >
                        </article>
                        <article>
                            <small>{l.pendingCount}</small><strong
                                >{snapshotView.pending.length.toString().padStart(2, '0')}</strong
                            >
                        </article>
                    </div>
                    <section>
                        <h2>{l.workflow}</h2>
                        <div class="three-columns">
                            {#each [1, 2, 3] as step}<article class="step">
                                    <span class="step-number">0{step}</span>
                                    <div class="heading-with-help">
                                        <h3>{l['step' + step]}</h3>
                                        <Info title={l['step' + step]} topic={step === 1 ? 'recovery' : 'signing'}
                                            ><p>{l['step' + step + 'Help']}</p></Info
                                        >
                                    </div>
                                </article>{/each}
                        </div>
                    </section>
                    <section>
                        <h2>{l.recent}</h2>
                        {#if snapshotView.history.length === 0}<p class="empty">
                                {l.emptyHistory}
                            </p>{:else}{#each snapshotView.history.slice(-5).reverse() as entry}<div class="list-row">
                                    <span class="badge">{entry.action}</span><code>{entry.txid ?? entry.digest}</code><small
                                        >{new Date(entry.at * 1000).toLocaleString()}</small
                                    >
                                </div>{/each}{/if}
                    </section>
                {:else if ['createWallet', 'wallets', 'passphrases'].includes(page)}{#key page}<Wallets
                            view={page}
                            {snapshotView}
                            {busy}
                            {run}
                        />{/key}
                {:else if page === 'multisig'}<Multisig {snapshotView} {busy} {run} />
                {:else if page === 'accounts'}<Accounts
                        {registry}
                        {snapshotView}
                        {busy}
                        {run}
                        onexport={() => void exportFile({ kind: 'public' })}
                    />
                {:else if page === 'transfer'}<Transfer
                        {registry}
                        {snapshotView}
                        {busy}
                        {run}
                        onprepared={() => {
                            page = 'requests';
                        }}
                    />
                {:else if page === 'requests'}
                    <section>
                        <div class="heading-with-help">
                            <h2>{l.multiPsbt}</h2>
                            <Info title={l.multiPsbt} topic="interoperability"><p>{l.multiPsbtHelp}</p></Info>
                        </div>
                        <div class="actions">
                            <label
                                >{l.network}<select bind:value={psbtNetwork}
                                    >{#each ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'] as network}<option
                                            value={network}>{network}</option
                                        >{/each}</select
                                ></label
                            >
                            <button disabled={busy} onclick={() => void importPsbt()}>{l.multiImportPsbt}</button>
                        </div>
                    </section>

                    {#if snapshotView.pending.length === 0}<section class="empty">
                            <p>{l.emptyRequests}</p>
                            <button disabled={busy} onclick={() => void importFile()}>{l.import}</button>
                        </section>{/if}
                    {#each snapshotView.pending as request}
                        {@const response = snapshotView.responses.find((r) => r.requestId === request.requestId)}
                        <section>
                            <div class="section-heading">
                                <h2>{l.native}</h2>
                                <span class="badge">{request.network}</span>
                            </div>
                            <p>{l.context}: {request.description}</p>
                            <code class="wrap">{request.requestId}</code>
                            {#if response}<div class="alert mid">{l.response}<code class="wrap">{response.txid}</code></div>{/if}
                            <div class="actions">
                                <button class="primary" onclick={() => void openReview({ id: request.requestId })} disabled={busy}
                                    >{response ? l.viewResponse : l.review}</button
                                ><button
                                    disabled={busy}
                                    onclick={() => {
                                        inspectorRequest = request.requestId;
                                        inspectorInitial = null;
                                        inspectorPath = '';
                                        page = 'inspector';
                                    }}>{l.inspectorInspect}</button
                                ><button
                                    onclick={() => void exportFile({ kind: 'request', id: request.requestId })}
                                    disabled={busy}>{l.exportRequest}</button
                                >{#if ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'].includes(request.network)}<button
                                        onclick={() => void exportFile({ kind: 'psbt', id: request.requestId })}
                                        disabled={busy}>{response ? l.exportSignedPsbt : l.multiExportPsbt}</button
                                    >{/if}
                                {#if response && ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'].includes(request.network)}<button
                                        onclick={() => void exportFile({ kind: 'transaction', id: request.requestId })}
                                        disabled={busy}>{l.exportRawTransaction}</button
                                    >{/if}
                                {#if response}<button
                                        onclick={() => void exportFile({ kind: 'response', id: request.requestId })}
                                        disabled={busy}>{l.exportResponse}</button
                                    >{/if}
                            </div>
                        </section>
                    {/each}
                {:else if page === 'inspector'}<PayloadInspector
                        initial={inspectorInitial}
                        initialPath={inspectorPath}
                        requestId={inspectorRequest}
                        bitcoin={snapshotView.pending.find((r) => r.requestId === inspectorRequest)?.payload.encoding ===
                            'base64' &&
                            ['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'].includes(
                                snapshotView.pending.find((r) => r.requestId === inspectorRequest)?.network ?? '',
                            )}
                        hasResponse={snapshotView.responses.some((r) => r.requestId === inspectorRequest)}
                    />
                    {#if inspectorRequest}<button
                            onclick={() => {
                                inspectorRequest = '';
                                inspectorInitial = null;
                                inspectorPath = '';
                            }}>{l.inspectorOpen}</button
                        >{/if}
                {:else if page === 'history'}
                    <section>
                        {#if !snapshotView.history.length}<p class="empty">{l.emptyHistory}</p>{:else}{#each snapshotView.history
                                .slice()
                                .reverse() as entry}<article class="history-entry">
                                    <div class="section-heading">
                                        <span class="badge">{entry.action}</span><small
                                            >{new Date(entry.at * 1000).toLocaleString()}</small
                                        >
                                    </div>
                                    <code class="wrap">{entry.requestId}</code><code class="wrap"
                                        >{entry.txid ?? entry.digest}</code
                                    >
                                </article>{/each}{/if}
                    </section>
                {:else if page === 'networks'}
                    <div class="three-columns chain-list">
                        {#each registry.filter((c) => !['testnet', 'testnet4', 'signet', 'regtest', 'sepolia', 'solana_devnet', 'cardano_preprod', 'tron_nile'].includes(c.network)) as item}
                            <section>
                                <h2>{item.name}</h2>
                                <span class="badge">{item.symbol}</span>{#if item.tokens}<span class="badge neutral"
                                        >{item.tokens}</span
                                    >{/if}
                            </section>
                        {/each}
                    </div>
                {:else if page === 'settings'}
                    <SettingsView {registry} {snapshotView} {busy} {run} />
                    <section>
                        <h2>{l.appearance}</h2>
                        <Appearance />
                    </section>
                {:else if page === 'help'}<Help bind:topic={helpTopic} />
                {:else if page === 'about'}<About />
                {/if}
            </main>
        </div>
    </div>
{/if}
{#if review && snapshotView.unlocked}<RequestReview
        {error}
        {snapshotView}
        {review}
        {busy}
        {run}
        onclose={() => {
            review = null;
        }}
    />{/if}

<dialog
    bind:this={aboutDialog}
    aria-labelledby="gate-about-title"
    class="about-dialog"
    onclose={() => {
        showAbout = false;
    }}
>
    <div class="dialog-heading">
        <h2 id="gate-about-title">{l.about}</h2>
        <button onclick={() => aboutDialog.close()}>{l.dismiss}</button>
    </div>
    {#if showAbout}<About />{/if}
</dialog>
