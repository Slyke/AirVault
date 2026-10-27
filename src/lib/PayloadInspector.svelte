<script lang="ts">
    import { untrack } from 'svelte';
    import { open } from '@tauri-apps/plugin-dialog';
    import { call, errorText, type PayloadReport } from './api';
    import { l } from './i18n';
    import Info from './Info.svelte';
    let {
        initial = null,
        initialPath = '',
        requestId = '',
        bitcoin = false,
        hasResponse = false,
    }: {
        initial?: PayloadReport | null;
        initialPath?: string;
        requestId?: string;
        bitcoin?: boolean;
        hasResponse?: boolean;
    } = $props();
    let report = $state<PayloadReport | null>(null);
    let path = $state('');
    let text = $state('');
    let hint = $state('auto');
    let kind = $state('request');
    let error = $state('');
    let notice = $state('');
    let busy = $state(false);
    let documentIndex = $state(0);
    let start = $state(0);
    let jump = $state('');
    let query = $state('');
    let active = $state(-1);
    let pinned = $state(-1);
    let cursor = $state(0);
    let generation = 0;
    const pageBytes = 256;
    const document = $derived(report?.documents[documentIndex]);
    const anyFailures = $derived(
        report?.documents.some((entry) => entry.checks.some((check) => check.status === 'fail')) ?? false,
    );
    const size = $derived((document?.hex.length ?? 0) / 2);
    const fieldIndex = $derived.by(() => {
        if (!document) return [];
        // Ranges are bounded and non-overlapping. Only materialize the visible page.
        return Array.from({ length: Math.min(pageBytes, Math.max(0, size - start)) }, (_, index) => {
            const offset = start + index;
            let lo = 0,
                hi = document.fields.length - 1;
            while (lo <= hi) {
                const mid = (lo + hi) >>> 1;
                const field = document.fields[mid];
                if (offset < field.offset) hi = mid - 1;
                else if (offset >= field.offset + field.length) lo = mid + 1;
                else return mid;
            }
            return -1;
        });
    });
    const rows = $derived(Array.from({ length: Math.ceil(fieldIndex.length / 16) }, (_, index) => start + index * 16));
    const selected = $derived(document?.fields[pinned >= 0 ? pinned : active]);
    const matching = $derived(
        (document?.fields ?? [])
            .map((field, index) => ({ field, index }))
            .filter(({ field }) =>
                query.trim()
                    ? (field.name + ' ' + field.value).toLowerCase().includes(query.toLowerCase().trim())
                    : field.offset < start + pageBytes && field.offset + field.length > start,
            ),
    );
    const resetView = () => {
        start = 0;
        cursor = 0;
        active = -1;
        pinned = -1;
        query = '';
        jump = '';
    };
    const accept = ({ value }: { value: PayloadReport }) => {
        report = value;
        documentIndex =
            value.documents.length > 1 && ['Text encoding', 'AirVault envelope'].includes(value.documents[0].protocol) ? 1 : 0;
        resetView();
    };
    $effect(() => {
        const value = initial,
            source = initialPath;
        if (value)
            untrack(() => {
                path = source;
                accept({ value });
            });
    });
    // The current view owns errors; never log payloads or IPC parameters.
    const load = async ({ action }: { action: () => Promise<PayloadReport> }) => {
        const current = ++generation;
        busy = true;
        error = '';
        notice = '';
        try {
            const value = await action();
            if (current === generation) accept({ value });
        } catch (cause) {
            if (current === generation) error = errorText({ error: cause });
        } finally {
            if (current === generation) busy = false;
        }
    };
    const file = async () => {
        error = '';
        try {
            const selected = await open({ multiple: false });
            if (typeof selected !== 'string') return;
            path = selected;
            await load({ action: () => call<PayloadReport>({ command: 'inspect_file', args: { path, hint } }) });
        } catch (cause) {
            error = errorText({ error: cause });
        }
    };
    const decode = () => load({ action: () => call<PayloadReport>({ command: 'inspect_text', args: { text, hint } }) });
    const reloadFile = () => load({ action: () => call<PayloadReport>({ command: 'inspect_file', args: { path, hint } }) });
    $effect(() => {
        if (requestId) {
            const id = requestId,
                selectedKind = kind;
            void load({ action: () => call<PayloadReport>({ command: 'inspect_exchange', args: { id, kind: selectedKind } }) });
        }
    });
    const movePage = ({ next }: { next: number }) => {
        start = Math.max(0, Math.min(Math.floor(Math.max(0, size - 1) / pageBytes) * pageBytes, next));
        cursor = start;
        active = -1;
    };
    const go = () => {
        const value = jump.trim();
        if (!/^(?:0x[0-9a-f]+|[0-9]+)$/i.test(value)) {
            error = l.inspectorOffsetError;
            return;
        }
        const offset = Number(value);
        if (!Number.isSafeInteger(offset) || offset < 0 || offset >= size) {
            error = l.inspectorOffsetError;
            return;
        }
        error = '';
        movePage({ next: Math.floor(offset / pageBytes) * pageBytes });
        cursor = offset;
        active = fieldIndex[offset - start] ?? -1;
    };
    const choose = ({ index }: { index: number }) => {
        const field = document?.fields[index];
        if (!field) return;
        pinned = index;
        active = index;
        movePage({ next: Math.floor(field.offset / pageBytes) * pageBytes });
        cursor = field.offset;
    };
    const byte = ({ offset }: { offset: number }) => document?.hex.slice(offset * 2, offset * 2 + 2) ?? '';
    const ascii = ({ offset }: { offset: number }) => {
        const value = parseInt(byte({ offset }), 16);
        return value >= 32 && value <= 126 ? String.fromCharCode(value) : '·';
    };
    const key = ({ event, offset, pane }: { event: KeyboardEvent; offset: number; pane: HTMLElement }) => {
        const delta: Record<string, number> = {
            ArrowLeft: -1,
            ArrowRight: 1,
            ArrowUp: -16,
            ArrowDown: 16,
            Home: start - offset,
            End: Math.min(start + pageBytes, size) - 1 - offset,
        };
        if (event.key === 'Escape') {
            pinned = -1;
            return;
        }
        if (!(event.key in delta)) return;
        event.preventDefault();
        cursor = Math.max(start, Math.min(Math.min(start + pageBytes, size) - 1, offset + delta[event.key]));
        active = fieldIndex[cursor - start] ?? -1;
        pane.closest('.inspector-hex-scroll')
            ?.querySelector<HTMLButtonElement>('[data-pane="' + pane.dataset.pane + '"][data-offset="' + cursor + '"]')
            ?.focus();
    };
    const copy = async () => {
        try {
            await navigator.clipboard.writeText(document?.hex ?? '');
            notice = l.copied;
        } catch {
            error = l.inspectorCopyFailed;
        }
    };
</script>

<div class="payload-inspector">
    {#if !requestId}
        <section class="inspector-input">
            <div class="heading-with-help">
                <h2>{l.inspectorOpen}</h2>
                <Info title={l.inspector} topic="inspector"><p>{l.inspectorHelp}</p></Info>
            </div>
            <div class="actions">
                <button class="primary" disabled={busy} onclick={() => void file()}>{l.inspectorFile}</button>
                {#if path}<span class="wrap">{path.split(/[\\/]/).at(-1)}</span>{/if}
            </div>
            <label
                >{l.inspectorDecoder}<select bind:value={hint}>
                    {#each ['auto', 'bitcoin-tx', 'psbt', 'bitcoin-p2p', 'evm'] as decoder}<option value={decoder}
                            >{l['decoder_' + decoder]}</option
                        >{/each}
                </select></label
            >
            {#if path}<button disabled={busy} onclick={() => void reloadFile()}>{l.inspectorDecodeFile}</button>{/if}
            <details>
                <summary>{l.inspectorPaste}</summary>
                <label
                    >{l.inspectorText}<textarea bind:value={text} spellcheck="false" maxlength={8 * 1024 * 1024}
                    ></textarea></label
                >
                <button disabled={busy || !text.length} onclick={() => void decode()}>{l.inspectorDecode}</button>
            </details>
            <p class="hint">{l.inspectorLimit}</p>
        </section>
    {:else}
        <label
            >{l.inspectorSource}<select bind:value={kind} disabled={busy}>
                <option value="request">{l.inspectorRequest}</option>
                {#if bitcoin}<option value="psbt">{l.inspectorCurrentPsbt}</option>{/if}
                {#if hasResponse}<option value="response">{l.inspectorResponse}</option>{/if}
            </select></label
        >
    {/if}
    {#if busy}<p role="status">{l.working}</p>{/if}
    {#if error}<p class="alert danger" role="alert">{error}</p>{/if}
    {#if notice}<p role="status">{notice}</p>{/if}
    {#if report && document}
        {#if anyFailures}<p class="alert danger" role="alert">{l.inspectorFailedChecks}</p>{/if}
        <section>
            <div class="section-heading">
                <h2>{document.protocol}</h2>
                {#if !requestId}<button
                        disabled={busy}
                        onclick={() => {
                            generation += 1;
                            busy = false;
                            report = null;
                            text = '';
                            path = '';
                        }}>{l.inspectorClear}</button
                    >{/if}
            </div>
            <dl class="inspector-properties">
                <dt>{l.inspectorFileHash}</dt>
                <dd><code class="wrap">{report.sourceSha256}</code></dd>
                <dt>{l.inspectorFileSize}</dt>
                <dd>{report.sourceSize.toLocaleString()} {l.inspectorBytes} · {report.encoding}</dd>
            </dl>
            <label
                >{l.inspectorDocument}<select
                    value={documentIndex}
                    onchange={(event) => {
                        documentIndex = Number(event.currentTarget.value);
                        resetView();
                    }}
                >
                    {#each report.documents as entry, index}<option value={index}
                            >{entry.name} · {entry.protocol} · {entry.hex.length / 2} {l.inspectorBytes}</option
                        >{/each}
                </select></label
            >
            <div class="inspector-checks" aria-label={l.inspectorChecks}>
                {#each document.checks as check}
                    <details class:inspector-fail={check.status === 'fail'} open={check.status === 'fail'}>
                        <summary
                            ><span class="badge" class:danger={check.status === 'fail'} class:neutral={check.status === 'info'}
                                >{l['inspector_' + check.status]}</span
                            >
                            {check.name}</summary
                        >
                        <p>{check.detail}</p>
                    </details>
                {/each}
            </div>
            {#if document.properties.length}<dl class="inspector-properties">
                    {#each document.properties as [name, value]}<dt>{name}</dt>
                        <dd><code class="wrap">{value}</code></dd>{/each}
                </dl>{/if}
        </section>
        <section class="inspector-view">
            <div class="section-heading">
                <h2>{l.inspectorBytesTitle}</h2>
                <button onclick={() => void copy()}>{l.inspectorCopyHex}</button>
            </div>
            <div class="inspector-controls">
                <button disabled={start === 0} onclick={() => movePage({ next: start - pageBytes })}>{l.inspectorPrevious}</button
                >
                <span
                    >{size ? start.toLocaleString() : 0}–{Math.min(start + pageBytes, size).toLocaleString()} / {size.toLocaleString()}</span
                >
                <button disabled={start + pageBytes >= size} onclick={() => movePage({ next: start + pageBytes })}
                    >{l.inspectorNext}</button
                >
                <form
                    onsubmit={(event) => {
                        event.preventDefault();
                        go();
                    }}
                >
                    <label>{l.inspectorOffset}<input bind:value={jump} placeholder="0x00" /></label><button
                        >{l.inspectorGo}</button
                    >
                </form>
            </div>
            <p class="hint">{l.inspectorHover}</p>
            <div class="inspector-legend">
                {#each ['header', 'input', 'output', 'witness', 'metadata', 'payload', 'opaque'] as group}<span data-group={group}
                        >{l['inspectorGroup_' + group]}</span
                    >{/each}
            </div>
            <div class="inspector-layout">
                <div class="inspector-hex-scroll">
                    <div class="inspector-hex-head">
                        <span>{l.inspectorOffset}</span><span>{l.inspectorHex}</span><span>ASCII</span>
                    </div>
                    {#each rows as row}
                        <div class="inspector-byte-row">
                            <code class="inspector-address">{row.toString(16).padStart(8, '0')}</code>
                            {#each ['hex', 'ascii'] as pane}
                                <div class="inspector-byte-pane" class:inspector-ascii={pane === 'ascii'} data-pane={pane}>
                                    {#each Array.from({ length: Math.min(16, size - row) }, (_, index) => row + index) as offset}
                                        {@const index = fieldIndex[offset - start]}
                                        {@const field = document.fields[index]}
                                        <button
                                            type="button"
                                            class="inspector-byte"
                                            class:inspector-byte-active={index >= 0 && index === (pinned >= 0 ? pinned : active)}
                                            data-group={field?.group ?? 'opaque'}
                                            data-offset={offset}
                                            data-pane={pane}
                                            aria-label={field?.name +
                                                ' · 0x' +
                                                offset.toString(16) +
                                                ' · ' +
                                                pane +
                                                ' ' +
                                                byte({ offset })}
                                            aria-pressed={pinned >= 0 && pinned === index}
                                            tabindex={offset === cursor ? 0 : -1}
                                            title={field?.name ?? l.inspectorUndecoded}
                                            onpointerenter={() => {
                                                active = index;
                                            }}
                                            onfocus={() => {
                                                active = index;
                                                cursor = offset;
                                            }}
                                            onclick={() => {
                                                pinned = pinned === index ? -1 : index;
                                                cursor = offset;
                                            }}
                                            onkeydown={(event) =>
                                                key({ event, offset, pane: event.currentTarget.parentElement! })}
                                            >{pane === 'hex' ? byte({ offset }) : ascii({ offset })}</button
                                        >
                                    {/each}
                                </div>
                            {/each}
                        </div>
                    {/each}
                    {#if !size}<p>{l.inspectorEmpty}</p>{/if}
                </div>
                <div class="inspector-helper">
                    <div class="section-heading">
                        <h3>{selected?.name ?? l.inspectorSelectField}</h3>
                        {#if pinned >= 0}<button
                                onclick={() => {
                                    pinned = -1;
                                }}>{l.inspectorUnpin}</button
                            >{/if}
                    </div>
                    {#if selected}
                        <p class="hint">
                            0x{selected.offset.toString(16)} · {selected.length.toLocaleString()}
                            {l.inspectorBytes}
                        </p>
                        <code class="wrap">{selected.value}</code>
                        <p>{selected.explanation}</p>
                        <h4>{l.inspectorExpected}</h4>
                        <p>{selected.expected}</p>
                    {:else}<p>{l.inspectorHover}</p>{/if}
                </div>
            </div>
            <label>{l.inspectorFindField}<input type="search" bind:value={query} /></label>
            <div class="inspector-field-list" aria-label={l.inspectorFields}>
                {#each matching.slice(0, 80) as { field, index }}<button
                        class:active={index === (pinned >= 0 ? pinned : active)}
                        onclick={() => choose({ index })}
                        ><code>0x{field.offset.toString(16).padStart(4, '0')}</code>
                        {field.name} <small>{field.length} B</small></button
                    >{/each}
            </div>
            {#if matching.length > 80}<p class="hint">{l.inspectorRefine}</p>{/if}
        </section>
    {/if}
</div>
