<script lang="ts">
    import { getContext, type Snippet } from 'svelte';
    import { l } from './i18n';
    import { helpContext, type HelpNavigation } from './help';
    let { title, children, topic }: { title: string; children: Snippet; topic?: string } = $props();
    let dialog: HTMLDialogElement;
    const id = $props.id();
    const navigation = getContext<HelpNavigation | undefined>(helpContext);
    const dismissBackdrop = (event: MouseEvent) => {
        const bounds = dialog.getBoundingClientRect();
        if (
            event.target === dialog &&
            (event.clientX < bounds.left ||
                event.clientX > bounds.right ||
                event.clientY < bounds.top ||
                event.clientY > bounds.bottom)
        )
            dialog.close();
    };
</script>

<button type="button" class="info-button" aria-label={title + ' — ' + l.moreInfo} onclick={() => dialog.showModal()}>i</button>
<dialog bind:this={dialog} aria-labelledby={id} class="info-dialog" onclick={dismissBackdrop}>
    <div class="dialog-heading">
        <h2 {id}>{title}</h2>
        <button type="button" onclick={() => dialog.close()}>{l.dismiss}</button>
    </div>
    {@render children()}
    {#if topic && navigation?.available()}
        <button
            type="button"
            class="learn-more"
            onclick={() => {
                dialog.close();
                navigation?.open(topic);
            }}>{l.learnMore}</button
        >
    {/if}
</dialog>
