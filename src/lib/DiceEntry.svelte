<script lang="ts">
    import { appendRoll, undoRoll, resetRolls, rollBit, diceWord, manualWord, worksheet, type DiceWord } from './dice';
    import { l } from './i18n';
    let {
        entries = $bindable(),
        rawRolls = $bindable(),
        count,
        bitCount,
        raw = false,
        disabled = false,
    }: { entries: DiceWord[]; rawRolls: number[]; count: number; bitCount: number; raw?: boolean; disabled?: boolean } = $props();
    let current = $state<number[]>([]);
    let inputWord = $state('');
    let error = $state('');
    const rolls = $derived(raw ? rawRolls : current);
    const limit = $derived(raw ? bitCount : 11);
    const preview = $derived(!raw && current.length === 11 ? diceWord({ rolls: current }) : null);
    const update = ({ next }: { next: number[] }) => {
        if (raw) rawRolls = next;
        else current = next;
        error = '';
    };
    const add = ({ roll }: { roll: number }) => update({ next: appendRoll({ rolls, roll, limit }) });
    const accept = () => {
        if (preview) {
            entries = [...entries, preview];
            current = [];
        }
    };
    const acceptManual = () => {
        try {
            entries = [...entries, manualWord({ word: inputWord })];
            inputWord = '';
            current = [];
            error = '';
        } catch {
            error = l.invalidWord;
        }
    };
    const previous = () => {
        const entry = entries.at(-1);
        if (!entry) return;
        current = [...entry.rolls];
        inputWord = entry.rolls.length ? '' : entry.word;
        entries = entries.slice(0, -1);
    };
</script>

<div class="dice-entry">
    <p>{raw ? l.fullDiceHelp : l.diceBasic}</p>
    <p class="hint">{l.fairDice}</p>
    {#if raw || entries.length < count}
        <h3>{raw ? l.entropyRolls : l.word + ' ' + (entries.length + 1)}</h3>
        <p>{rolls.length} / {limit} {l.rollsEntered}</p>
        <div class="dice-buttons" aria-label={l.enterRoll}>
            {#each [1, 2, 3, 4, 5, 6] as roll}<button
                    disabled={disabled || rolls.length >= limit}
                    aria-label={l.roll + ' ' + roll}
                    onclick={() => add({ roll })}>{roll}</button
                >{/each}
        </div>
        <p class="hint">{l.diceRolls}: {rolls.join(' ') || '—'}</p>
        <div class="dice-bits" aria-live="polite">{rolls.map((roll) => rollBit({ roll })).join(' ') || '—'}</div>
        <div class="actions">
            <button disabled={disabled || !rolls.length} onclick={() => update({ next: undoRoll({ rolls }) })}
                >{l.undoRoll}</button
            ><button disabled={disabled || !rolls.length} onclick={() => update({ next: resetRolls() })}
                >{raw ? l.resetEntropy : l.resetWord}</button
            >
        </div>
        {#if preview}
            <div class="alert mid">
                <strong>{preview.word}</strong>
                <p>{l.zeroIndex}: {preview.index} · {l.printPosition}: {preview.position}</p>
            </div>
            <details>
                <summary>{l.calculated}</summary>
                <pre>{worksheet({ entry: preview })}</pre>
            </details>
            <div class="dice-accept"><button class="primary" {disabled} onclick={accept}>{l.acceptWord}</button></div>
        {/if}
        {#if !raw}<div class="dice-alternative">
                <details>
                    <summary>{l.alreadyGenerated}</summary><label
                        >{l.englishWord}<input
                            bind:value={inputWord}
                            autocomplete="off"
                            spellcheck="false"
                            maxlength="16"
                        /></label
                    ><button disabled={disabled || !inputWord} onclick={acceptManual}>{l.addWord}</button>
                </details>
            </div>{/if}
    {/if}
    {#if !raw && entries.length}<button {disabled} onclick={previous}>{l.editLastWord}</button>{/if}
    {#if error}<div role="alert" class="alert danger">{error}</div>{/if}
</div>
