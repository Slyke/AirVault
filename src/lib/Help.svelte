<script lang="ts">
    import { tick } from 'svelte';
    import { save } from '@tauri-apps/plugin-dialog';
    import { call, errorText } from './api';
    import { l } from './i18n';
    import { helpGroups, helpTopics, relatedTopics } from './help';
    import DiceHelp from './DiceHelp.svelte';
    let { topic = $bindable('start') }: { topic?: string } = $props();
    let level = $state('basic');
    let query = $state('');
    let history = $state<string[]>([]);
    let article: HTMLElement;
    let saving = $state(false);
    let message = $state('');
    let failed = $state(false);
    const current = $derived(helpTopics.includes(topic) ? topic : 'start');
    const matches = $derived(
        helpTopics.filter((item) =>
            ['topic_', 'guide_', 'steps_', 'result_', 'note_']
                .map((prefix) =>
                    [prefix + item, prefix + item + '_basic', prefix + item + '_advanced'].map((key) => l[key] ?? '').join(' '),
                )
                .join(' ')
                .toLowerCase()
                .includes(query.trim().toLowerCase()),
        ),
    );
    const go = async ({ next, back = false }: { next: string; back?: boolean }) => {
        if (!helpTopics.includes(next)) return;
        if (!back && next !== current) history = [...history, current];
        topic = next;
        query = '';
        message = '';
        await tick();
        article.focus({ preventScroll: true });
        article.scrollIntoView({ block: 'start', behavior: 'instant' });
    };
    const previous = () => {
        const next = history.at(-1);
        if (!next) return;
        history = history.slice(0, -1);
        void go({ next, back: true });
    };
    const whitepaper = async () => {
        saving = true;
        message = '';
        failed = false;
        try {
            const path = await save({
                defaultPath: 'bitcoin-whitepaper.pdf',
                filters: [{ name: l.pdfDocument, extensions: ['pdf'] }],
            });
            if (path) {
                await call({ command: 'save_whitepaper', args: { path } });
                message = l.whitepaperSaved;
            }
        } catch (error) {
            message = errorText({ error });
            failed = true;
        } finally {
            saving = false;
        }
    };
</script>

<div class="wiki-layout">
    <div class="wiki-index">
        <label for="wiki-search">{l.searchInstructions}</label>
        <input id="wiki-search" type="search" bind:value={query} placeholder={l.searchInstructionsHint} />
        {#if query.trim()}<p class="hint" role="status">{matches.length} {l.articlesFound}</p>{/if}
        <nav class="wiki-topics" aria-label={l.instructionTopics}>
            {#each Object.entries(helpGroups) as [group, topics]}
                {#if topics.some((item) => matches.includes(item))}
                    <div class="wiki-group">
                        <h2>{l['group_' + group]}</h2>
                        {#each topics.filter((item) => matches.includes(item)) as item}
                            <button
                                class:active={current === item}
                                aria-current={current === item ? 'page' : undefined}
                                onclick={() => void go({ next: item })}>{l['topic_' + item]}</button
                            >
                        {/each}
                    </div>
                {/if}
            {/each}
            {#if !matches.length}<p>{l.noArticles}</p>{/if}
        </nav>
    </div>
    <div class="wiki-reading">
        <div class="wiki-toolbar">
            <button disabled={!history.length} onclick={previous}>{l.previousArticle}</button>
            <div class="tabs" aria-label={l.readingLevel}>
                <button
                    class:active={level === 'basic'}
                    aria-pressed={level === 'basic'}
                    onclick={() => {
                        level = 'basic';
                    }}>{l.basicInstructions}</button
                >
                <button
                    class:active={level === 'advanced'}
                    aria-pressed={level === 'advanced'}
                    onclick={() => {
                        level = 'advanced';
                    }}>{l.advancedInstructions}</button
                >
            </div>
        </div>
        <section class="wiki-article" bind:this={article} tabindex="-1" aria-labelledby="wiki-article-title">
            <div class="eyebrow">
                {l[
                    'group_' +
                        (Object.keys(helpGroups).find((group) =>
                            helpGroups[group as keyof typeof helpGroups].includes(current),
                        ) ?? 'essentials')
                ]}
            </div>
            <h2 id="wiki-article-title">{l['topic_' + current]}</h2>
            {#each l['guide_' + current + '_' + level].split('\n\n') as paragraph}<p>{paragraph}</p>{/each}
            {#if level === 'basic' && l['steps_' + current]}
                <h3>{l.followTheseSteps}</h3>
                <ol class="wiki-steps">
                    {#each l['steps_' + current].split('\n') as step}<li>{step}</li>{/each}
                </ol>
                <div class="wiki-result">
                    <h3>{l.whatToExpect}</h3>
                    <p>{l['result_' + current]}</p>
                </div>
            {/if}
            {#if l['note_' + current]}<details class="wiki-note">
                    <summary>{l.checkBeforeContinuing}</summary>
                    <p>{l['note_' + current]}</p>
                </details>{/if}
            {#if current === 'dice'}<DiceHelp />{/if}
            {#if current === 'whitepaper'}
                <div class="reference-download">
                    <h3>{l.whitepaperTitle}</h3>
                    <p>{l.whitepaperAuthor}</p>
                    <button class="primary" disabled={saving} onclick={() => void whitepaper()}
                        >{saving ? l.working : l.saveWhitepaper}</button
                    >
                    <p class="hint">{l.whitepaperOffline}</p>
                    <a href="https://bitcoin.org/bitcoin.pdf" target="_blank" rel="noreferrer">{l.originalSource}</a>
                    <p class="hint">{l.referenceSha256}</p>
                    <code class="wrap">b1674191a88ec5cdd733e4240a81803105dc412d6c6708d53ab94fc248f4f553</code>
                    {#if message}<p role={failed ? 'alert' : 'status'}>{message}</p>{/if}
                </div>
            {/if}
            <footer class="wiki-related">
                <h3>{l.relatedArticles}</h3>
                <div class="actions">
                    {#each relatedTopics[current] as next}<button onclick={() => void go({ next })}>{l['topic_' + next]}</button
                        >{/each}
                </div>
            </footer>
        </section>
    </div>
</div>
