<script lang="ts">
    import { l } from './i18n';
    import { diceWord, worksheet, wordlist } from './dice';
    let query = $state('');
    const example = diceWord({ rolls: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 6] });
    const matches = $derived(
        wordlist
            .map((word, index) => ({ word, index }))
            .filter(
                ({ word, index }) => !query || word.startsWith(query.trim().toLowerCase()) || String(index + 1) === query.trim(),
            ),
    );
</script>

<div class="help-page">
    <h2>{l.guideTitle}</h2>
    <p>{l.helpOffline}</p>
    <pre>{l.manualSteps}</pre>
    <p>{l.independentList}</p>
    <p>
        <a href="https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt" target="_blank" rel="noopener noreferrer"
            >{l.officialList}</a
        >
    </p>
    <details>
        <summary>{l.publicExample}</summary>
        <pre>{worksheet({ entry: example })}</pre>
    </details>
    <details>
        <summary>{l.advancedInstructions}</summary>
        <p>{l.advancedEntropy}</p>
        <p>{l.advancedPrefix}</p>
        <p>{l.fairDice}</p>
    </details>
    <details>
        <summary>{l.localList}</summary><label
            >{l.findWord}<input bind:value={query} autocomplete="off" spellcheck="false" /></label
        >
        <div class="dice-review">
            <table>
                <thead><tr><th>{l.zeroIndex}</th><th>{l.printPosition}</th><th>{l.word}</th></tr></thead><tbody
                    >{#each matches as item}<tr><td>{item.index}</td><td>{item.index + 1}</td><td>{item.word}</td></tr
                        >{/each}</tbody
                >
            </table>
        </div>
    </details>
    <details>
        <summary>{l.blankWorksheet}</summary>
        <pre>{l.blankSheet}</pre>
    </details>
</div>
