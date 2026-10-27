<script lang="ts">
    import { untrack } from 'svelte';
    import { save } from '@tauri-apps/plugin-dialog';
    import { call, type Snapshot, type Settings, type Capability } from './api';
    import Info from './Info.svelte';
    import PasswordStrength from './PasswordStrength.svelte';
    import PasswordMatch from './PasswordMatch.svelte';
    import { l } from './i18n';
    let {
        snapshotView,
        registry,
        busy,
        run,
    }: {
        snapshotView: Snapshot;
        registry: Capability[];
        busy: boolean;
        run: (p: { action: () => Promise<unknown>; success?: string }) => Promise<void>;
    } = $props();
    let settings = $state<Settings>(
        untrack(() => ({ ...snapshotView.settings!, providers: { ...snapshotView.settings!.providers } })),
    );
    let providerNetwork = $state('bitcoin');
    let password = $state(''),
        confirmed = $state(false),
        backupConfirmed = $state(false),
        oldPassword = $state(''),
        newPassword = $state(''),
        confirmation = $state('');
    const saveSettings = () =>
        run({
            action: async () => {
                try {
                    await call({ command: 'save_settings', args: { settings, password, confirmed } });
                } finally {
                    password = '';
                    confirmed = false;
                }
            },
            success: l.saved,
        });
    const backup = () =>
        run({
            action: async () => {
                try {
                    const path = await save({
                        defaultPath: 'airvault-backup.avvault',
                        filters: [{ name: 'Encrypted AirVault backup', extensions: ['avvault'] }],
                    });
                    if (path) await call({ command: 'create_backup', args: { path, password, confirmed: backupConfirmed } });
                } finally {
                    password = '';
                    backupConfirmed = false;
                }
            },
            success: l.backupSaved,
        });
    const changePassword = () =>
        run({
            action: async () => {
                try {
                    if (newPassword !== confirmation) throw new Error(l.wrongConfirmation);
                    await call({ command: 'change_password', args: { oldPassword, newPassword } });
                } finally {
                    oldPassword = '';
                    newPassword = '';
                    confirmation = '';
                }
            },
            success: l.saved,
        });
</script>

<div class="two-columns align-start">
    <div>
        <section>
            <h2>{l.security}</h2>
            <div class="field">
                <div class="heading-with-help">
                    <label for="settings-mode">{l.mode}</label><Info title={l.mode} topic="modes">
                        <p><strong>{l.offline}</strong> — {l.modeOfflineHelp}</p>
                        <p><strong>{l.combined}</strong> — {l.modeOnlineHelp}</p>
                        <p><strong>{l.proxy}</strong> — {l.modeProxyHelp}</p>
                        {#if !snapshotView.networkBuild}<p class="alert warning">{l.proxyDisabledBuild}</p>
                        {:else if snapshotView.roots.length > 0}<p class="alert warning">{l.proxyDisabledRoots}</p>{/if}</Info
                    >
                </div>
                <select id="settings-mode" bind:value={settings.mode}
                    ><option value="offline">{l.offline}</option><option
                        value="proxy"
                        disabled={!snapshotView.networkBuild || snapshotView.roots.length > 0}>{l.proxy}</option
                    ><option value="combined" disabled={!snapshotView.networkBuild}>{l.combined}</option></select
                >
            </div>

            <div class="field">
                <div class="heading-with-help">
                    <label for="settings-timeout">{l.timeout}</label><Info title={l.timeout} topic="locking"
                        ><p>{l.timeoutHelp}</p></Info
                    >
                </div>
                <input
                    id="settings-timeout"
                    type="number"
                    min="-1"
                    max="3600"
                    step="1"
                    bind:value={settings.timeoutSeconds}
                    aria-describedby="timeout-hint"
                />
                <p id="timeout-hint" class="hint">{l.timeoutHint}</p>
            </div>
            <label class="check"><input type="checkbox" bind:checked={settings.lockOnBlur} />{l.blur}</label>
            <label
                >{l.providerNetwork}<select bind:value={providerNetwork}
                    >{#each registry as item}<option value={item.network}>{item.name}</option>{/each}</select
                ></label
            >
            <div class="field">
                <div class="heading-with-help">
                    <label for="settings-provider">{l.provider}</label><Info title={l.provider} topic="providers"
                        ><p>{l.providerHelp}</p></Info
                    >
                </div>
                <input
                    id="settings-provider"
                    bind:value={settings.providers[providerNetwork]}
                    placeholder={providerNetwork === 'polkadot' ? 'wss://…' : 'https://…'}
                />
            </div>

            <label>{l.password}<input type="password" bind:value={password} autocomplete="off" /></label><label class="check"
                ><input type="checkbox" bind:checked={confirmed} />{l.settingsConfirm}</label
            ><button class="primary" disabled={busy || !confirmed || !password} onclick={() => void saveSettings()}
                >{l.save}</button
            >
        </section>
    </div>
    <div>
        <section>
            <div class="heading-with-help">
                <h2>{l.backup}</h2>
                <Info title={l.backup} topic="backup"><p>{l.backupHelp}</p></Info>
            </div>
            <label>{l.password}<input type="password" bind:value={password} autocomplete="off" /></label><label class="check"
                ><input type="checkbox" bind:checked={backupConfirmed} />{l.backupCheck}</label
            ><button disabled={busy || !backupConfirmed || !password} onclick={() => void backup()}>{l.backupAction}</button>
        </section>
        <section>
            <h2>{l.changePassword}</h2>
            <label>{l.oldPassword}<input type="password" bind:value={oldPassword} autocomplete="off" /></label><label
                >{l.newPassword}<input type="password" bind:value={newPassword} autocomplete="off" minlength="6" /></label
            ><PasswordStrength password={newPassword} /><label
                >{l.passwordConfirm}<input type="password" bind:value={confirmation} autocomplete="off" /></label
            ><PasswordMatch password={newPassword} {confirmation} /><button
                disabled={busy || [...newPassword].length < 6 || !oldPassword || newPassword !== confirmation}
                onclick={() => void changePassword()}>{l.changePassword}</button
            >
        </section>
    </div>
</div>
