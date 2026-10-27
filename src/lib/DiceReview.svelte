<script lang="ts">
    import { worksheet, type DiceWord } from './dice';
    import { l } from './i18n';
    let { entries = $bindable(), count }: { entries: DiceWord[]; count: number } = $props();
</script>

{#if entries.length}<div class="dice-review-section">
        <h2>{l.reviewRecordedWords}</h2>
        <p role="status">{entries.filter((entry) => entry.verified).length} {l.of} {count} {l.wordsVerified}</p>
        <p class="hint">{l.checkboxHelp}</p>
        <div class="dice-review">
            {#each entries as entry, index}
                <article class="dice-word">
                    <h3>{l.word} {index + 1}: {entry.word}</h3>
                    <p class="hint">
                        {l.diceRolls}: {entry.rolls.length ? entry.rolls.join(' ') : l.manualEntry}<br />{l.binaryValue}: {entry.bits}<br
                        />{l.zeroIndex}: {entry.index} · {l.printPosition}: {entry.position}
                    </p>
                    <details>
                        <summary>{l.calculated}</summary>
                        <pre>{worksheet({ entry })}</pre>
                    </details>
                    <label class="check"><input type="checkbox" bind:checked={entry.verified} />{l.verifiedWord}</label>
                </article>
            {/each}
        </div>
    </div>{/if}
