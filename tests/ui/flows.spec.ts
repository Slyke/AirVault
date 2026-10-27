import { test, expect, type Page, type APIRequestContext } from '@playwright/test';
const bridge = async ({ request, command, args = {} }: { request: APIRequestContext; command: string; args?: object }) => {
    const response = await request.post('/__airvault_test', { data: { command, args } });
    const body = await response.json();
    expect(body.ok).toBe(true);
    return body.value;
};
const create = async ({ page }: { page: Page }) => {
    await page.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await page.getByLabel('Confirm password', { exact: true }).fill('public-test-password');
    await page.getByRole('button', { name: 'Create vault', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Lock vault' })).toBeVisible();
};
test.beforeEach(async ({ page, request }) => {
    await bridge({ request, command: 'test_reset' });
    await page.goto('/tests/ui/index.html');
});
test('three styleguide palettes, both modes, persistence and offline instructions', async ({ page }) => {
    await page.getByText('Appearance', { exact: true }).click();
    for (const palette of ['terminal', 'paper', 'ocean']) {
        await page.getByRole('combobox', { name: 'Theme', exact: true }).selectOption(palette);
        for (const mode of ['light', 'dark']) {
            await page.getByRole('combobox', { name: 'Mode', exact: true }).selectOption(mode);
            await expect(page.locator('html')).toHaveAttribute('data-palette', palette);
            await expect(page.locator('html')).toHaveAttribute('data-theme', mode);
        }
    }
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-palette', 'ocean');
    await create({ page });
    await page.getByRole('button', { name: 'Instructions', exact: true }).click();
    await page.getByRole('button', { name: 'Physical dice', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'How to verify your dice-generated BIP39 words manually' })).toBeVisible();
    await page.getByRole('button', { name: 'Advanced instructions', exact: true }).click();
    await expect(page.getByText(/The mapping is unbiased/)).toBeVisible();
    await page.getByText('Bundled offline wordlist', { exact: true }).click();
    await page.getByLabel('Find a word or printed position').fill('2048');
    await expect(page.getByRole('cell', { name: 'zoo', exact: true })).toBeVisible();
});
test('dice prefix review, undo, reset, one-time phrase and clearing on confirmation', async ({ page }) => {
    await create({ page });
    await page.getByRole('button', { name: 'Create or import', exact: true }).click();
    await page.getByLabel('Wallet label', { exact: true }).fill('Disposable dice wallet');
    await page.getByLabel('Manual prefix words', { exact: true }).fill('1');
    await page.getByLabel('Manual prefix words', { exact: true }).blur();
    await page.getByRole('button', { name: 'Roll 6', exact: true }).click();
    await page.getByRole('button', { name: 'Undo last roll', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Undo last roll', exact: true })).toBeDisabled();
    await page.getByRole('button', { name: 'Roll 2', exact: true }).click();
    await page.getByRole('button', { name: 'Reset current word', exact: true }).click();
    for (let i = 0; i < 10; i++) await page.getByRole('button', { name: 'Roll 1', exact: true }).click();
    await page.getByRole('button', { name: 'Roll 6', exact: true }).click();
    await page.getByText('(i) How was this word calculated?', { exact: true }).click();
    await expect(page.locator('.dice-entry pre')).toContainText('Zero-based BIP39 index: 1');
    await expect(page.locator('.dice-entry pre')).toContainText('BIP39 English word: ability');
    await page.getByRole('button', { name: 'Add this word', exact: true }).click();
    await page.getByLabel('I verified this word', { exact: true }).check();
    await expect(page.getByText('1 of 1 manually generated words verified.', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Generate recovery phrase', exact: true }).click();
    await expect(page.locator('.phrase-grid li')).toHaveCount(24);
    const words = await page.locator('.phrase-grid li').allTextContents();
    const phrase = words.map((word) => word.replace(/^\d+/, '').trim()).join(' ');
    expect(phrase.split(' ')).toHaveLength(24);
    expect(phrase.startsWith('ability ')).toBe(true);
    await page.getByRole('button', { name: 'I recorded it — hide and verify', exact: true }).click();
    await expect(page.locator('.phrase-grid')).toHaveCount(0);
    await expect(page.locator('.dice-review')).toBeVisible();
    await page.getByLabel('Re-enter all recovery words', { exact: true }).fill(phrase);
    await page.getByRole('button', { name: 'Confirm and save wallet', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Generate recovery phrase', exact: true })).toBeVisible();
    await expect(page.locator('.dice-review')).toHaveCount(1); // Help contains the public wordlist; actual entry review is gone.
    await expect(page.getByLabel('I verified this word', { exact: true })).toHaveCount(0);
    expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(phrase);
    await page.getByRole('button', { name: 'Wallets & recovery', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Disposable dice wallet', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Lock vault', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Open your vault' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Disposable dice wallet', exact: true })).toHaveCount(0);
});
test('all physical dice bits create a checksummed 12-word phrase', async ({ page }) => {
    await create({ page });
    await page.getByRole('button', { name: 'Create or import', exact: true }).click();
    await page.getByLabel('Wallet label', { exact: true }).fill('Raw dice test');
    await page.getByRole('combobox', { name: 'Phrase length', exact: true }).selectOption('12');
    await page.getByRole('combobox', { name: 'Entropy source', exact: true }).selectOption('raw');
    for (let i = 0; i < 128; i++) await page.getByRole('button', { name: 'Roll 3', exact: true }).click();
    await page.getByRole('button', { name: 'Generate recovery phrase', exact: true }).click();
    await expect(page.locator('.phrase-grid li').last()).toHaveText('12about');
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.locator('.phrase-grid')).toHaveCount(0);
    await expect(page.getByText('0 / 128 rolls entered', { exact: true })).toBeVisible();
});
test('actual native signing requires review, password and explicit checkbox', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await bridge({ request, command: 'test_fund' });
    await page.reload();
    await page.getByRole('button', { name: 'Signing requests', exact: true }).click();
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('40,000');
    await expect(dialog.getByRole('button', { name: 'Approve & sign' })).toBeDisabled();
    await dialog.getByLabel('Vault password', { exact: true }).fill('wrong-password');
    await dialog.getByLabel('I verified the network, every recipient, the amounts, and the fee.', { exact: true }).check();
    await dialog.getByRole('button', { name: 'Approve & sign', exact: true }).click();
    await expect(dialog.getByRole('alert')).toBeVisible();
    let snapshot = await bridge({ request, command: 'snapshot' });
    expect(snapshot.responses).toHaveLength(0);
    await dialog.getByRole('button', { name: 'Close review', exact: true }).click();
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    await dialog.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await dialog.getByLabel('I verified the network, every recipient, the amounts, and the fee.', { exact: true }).check();
    await dialog.getByRole('button', { name: 'Approve & sign', exact: true }).click();
    await expect(page.getByText('Signed and independently verified.', { exact: true })).toBeVisible();
    snapshot = await bridge({ request, command: 'snapshot' });
    expect(snapshot.responses).toHaveLength(1);
    expect(snapshot.history[0].action).toBe('signed');
    await page.getByRole('button', { name: 'Export signed PSBT', exact: true }).click();
    await expect(page.getByRole('status')).toHaveText('File exported.');
    expect(await bridge({ request, command: 'test_saved_psbt' })).toEqual([1]);
    await page.getByRole('button', { name: 'Export raw transaction', exact: true }).click();
    await expect(page.getByRole('status')).toHaveText('File exported.');
    const exported = await bridge({ request, command: 'test_saved_transaction' });
    expect(exported.txid).toBe(snapshot.responses[0].txid);
    expect(exported.witnesses).toEqual([2]);

    await page.getByRole('button', { name: 'Lock vault', exact: true }).click();
    snapshot = await bridge({ request, command: 'snapshot' });
    expect(snapshot.accounts).toHaveLength(0);
});

test('compact setup accepts a weak six-character password and confirms only matching', async ({ page, request }) => {
    await expect(page.getByText('Instructions', { exact: true })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight)).toBe(true);
    await page.getByLabel('Vault password', { exact: true }).fill('12345');
    await page.getByLabel('Confirm password', { exact: true }).fill('12345');
    await page.getByRole('button', { name: 'Create vault', exact: true }).click();
    expect((await bridge({ request, command: 'snapshot' })).initialized).toBe(false);
    await page.getByLabel('Vault password', { exact: true }).fill('123456');
    await expect(page.locator('.password-feedback')).toContainText('Very weak');
    await expect(page.locator('.password-feedback')).toContainText('Uppercase');
    await expect(page.locator('.password-match')).toHaveText('Passwords do not match');
    await page.getByLabel('Confirm password', { exact: true }).fill('123456');
    await expect(page.locator('.password-match')).toHaveText('Passwords match');
    await expect(page.getByRole('meter')).toHaveCount(1);
    await page.getByRole('button', { name: 'Create vault', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Lock vault' })).toBeVisible();
    expect((await bridge({ request, command: 'snapshot' })).settings.mode).toBe('offline');
    await page.getByRole('button', { name: 'Lock vault' }).click();
    await expect(page.getByRole('heading', { name: 'Open your vault' })).toBeVisible();
    await expect(page.getByText('Restore encrypted backup', { exact: true })).toHaveCount(0);
    await expect(page.getByRole('meter')).toHaveCount(0);
});
test('scale control, keyboard shortcuts and saved preference agree', async ({ page }) => {
    await page.getByText('Appearance', { exact: true }).click();
    const scale = page.getByRole('combobox', { name: 'Interface scale', exact: true });
    await scale.selectOption('120');
    await expect(page.locator('html')).toHaveCSS('zoom', '1.2');
    await page.keyboard.press('Control+-');
    await expect(scale).toHaveValue('110');
    await page.keyboard.press('Control+=');
    await expect(scale).toHaveValue('120');
    await page.reload();
    await page.getByText('Appearance', { exact: true }).click();
    await expect(scale).toHaveValue('120');
    await page.keyboard.press('Control+0');
    await expect(scale).toHaveValue('100');
    await create({ page });
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.keyboard.press('Control+=');
    await expect(page.getByRole('combobox', { name: 'Interface scale', exact: true })).toHaveValue('110');
});
test('all blockchain accounts derive from the UI with native defaults', async ({ page, request }) => {
    test.setTimeout(180_000);
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    const snapshot = await bridge({ request, command: 'snapshot' });
    const registry = await bridge({ request, command: 'capabilities' });
    await page.getByRole('button', { name: 'Accounts', exact: true }).click();
    await page.getByRole('combobox', { name: 'Signing identity', exact: true }).selectOption(snapshot.identities[0].id);
    for (const chain of registry.filter(
        (c: { network: string }) => !['bitcoin', 'testnet', 'testnet4', 'signet', 'regtest'].includes(c.network),
    )) {
        await page.getByRole('combobox', { name: 'Network', exact: true }).selectOption(chain.network);
        await expect(page.getByRole('textbox', { name: 'Derivation path', exact: true })).toHaveValue(chain.path);
        await page.getByLabel('Label', { exact: true }).fill(chain.name + ' fixture');
        await page.getByRole('button', { name: 'Derive account', exact: true }).click();
        await expect(page.getByRole('heading', { name: chain.name + ' fixture', exact: true })).toBeVisible();
    }
    expect((await bridge({ request, command: 'snapshot' })).accounts).toHaveLength(21);
    await page.getByRole('button', { name: 'Cryptocurrencies', exact: true }).click();
    await expect(page.getByText(/unsupported|tested chain-specific adapter|This Windows preview/i)).toHaveCount(0);
    await expect(page.getByText('TKN', { exact: true })).toHaveCount(0);
});
test('create an ERC-20 account, add another token, and approve its exact transfer', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    const initial = await bridge({ request, command: 'snapshot' });
    await page.getByRole('button', { name: 'Accounts', exact: true }).click();
    await page.getByRole('combobox', { name: 'Signing identity', exact: true }).selectOption(initial.identities[0].id);
    await page.getByRole('combobox', { name: 'Network', exact: true }).selectOption('ethereum');
    await page.getByLabel('Label', { exact: true }).fill('Ethereum tokens');
    await page.getByRole('checkbox', { name: 'Include a token', exact: false }).check();
    await page.getByLabel('Contract / mint address', { exact: true }).fill('0x1111111111111111111111111111111111111111');
    await page.getByLabel('Token symbol', { exact: true }).fill('TKN');
    await page.getByRole('button', { name: 'Derive account', exact: true }).click();
    const accountSection = page
        .locator('section')
        .filter({ has: page.getByRole('heading', { name: 'Ethereum tokens', exact: true }) });
    await expect(accountSection).toContainText('TKN');
    await accountSection.getByRole('button', { name: 'Include a token', exact: true }).click();
    await accountSection
        .getByLabel('Contract / mint address', { exact: true })
        .fill('0x2222222222222222222222222222222222222222');
    await accountSection.getByLabel('Token symbol', { exact: true }).fill('USDC');
    await accountSection.getByLabel('Decimals', { exact: true }).fill('6');
    await accountSection.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(accountSection).toContainText('USDC');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    const security = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Security', exact: true }) });
    await security.getByRole('combobox', { name: 'Operating mode', exact: true }).selectOption('combined');
    await security.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await security.getByLabel('I confirm these security and mode changes.', { exact: true }).check();
    await security.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.locator('.vault-state')).toContainText('Online');
    await page.getByRole('button', { name: 'Prepare transfer', exact: true }).click();
    const snapshot = await bridge({ request, command: 'snapshot' });
    const account = snapshot.accounts.find((a: { label: string }) => a.label === 'Ethereum tokens');
    await page.getByRole('combobox', { name: 'Source account', exact: true }).selectOption(account.id);
    await page.getByRole('combobox', { name: 'Asset', exact: true }).selectOption('0');
    await page.getByLabel('Recipient address', { exact: true }).fill('0x3333333333333333333333333333333333333333');
    await page.getByLabel('Amount · TKN', { exact: true }).fill('100');
    await page.getByRole('button', { name: 'Prepare signing request', exact: true }).click();
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    const dialog = page.getByRole('dialog').filter({ has: page.getByRole('button', { name: 'Approve & sign', exact: true }) });
    await expect(dialog).toContainText('100 TKN');
    await expect(dialog).toContainText('0x1111111111111111111111111111111111111111');
    await dialog.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await dialog.getByLabel('I verified the network, every recipient, the amounts, and the fee.', { exact: true }).check();
    await dialog.getByRole('button', { name: 'Approve & sign', exact: true }).click();
    await expect(page.getByText('Signed and independently verified.', { exact: true })).toBeVisible();
    expect((await bridge({ request, command: 'snapshot' })).responses).toHaveLength(1);
});

const publicWords = [...Array(11).fill('abandon'), 'about'];
const numberedFixture = publicWords.map((word, index) => `${index + 1}. ${word}`).join('\n');
const publicPrefixes = publicWords.map((word) => word.slice(0, 4)).join(' ');
const generatePublicFixture = async ({ page }: { page: Page }) => {
    await create({ page });
    await page.getByRole('button', { name: 'Create or import', exact: true }).click();
    await page.getByRole('textbox', { name: 'Wallet label', exact: true }).fill('Public recovery fixture');
    await page.getByRole('combobox', { name: 'Phrase length', exact: true }).selectOption('12');
    await page.getByRole('combobox', { name: 'Entropy source', exact: true }).selectOption('raw');
    await page.getByRole('combobox', { name: 'Entropy entry', exact: true }).selectOption('bits');
    await page.getByRole('textbox', { name: 'Verified raw entropy bits', exact: true }).fill('0'.repeat(128));
    await page.getByRole('button', { name: 'Generate recovery phrase', exact: true }).click();
    await expect(page.locator('.phrase-grid li')).toHaveCount(12);
};
test('copy requires confirmation and emits a numbered phrase with one word per line', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await generatePublicFixture({ page });
    await page.evaluate(() => navigator.clipboard.writeText('unchanged'));
    await page.getByRole('button', { name: 'Copy seed phrase', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Copy your seed phrase?', exact: true });
    await expect(dialog).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('unchanged');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('unchanged');
    await page.getByRole('button', { name: 'Copy seed phrase', exact: true }).click();
    await dialog.getByRole('button', { name: 'Yes, copy seed phrase', exact: true }).click();
    await expect(dialog).not.toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(numberedFixture);
    await page.getByRole('button', { name: 'I recorded it — hide and verify', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Copy seed phrase', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.evaluate(() => navigator.clipboard.writeText(''));
});
test('recovery confirmation keeps errors editable and accepts numbered four-letter words on retry', async ({ page, request }) => {
    await generatePublicFixture({ page });
    await page.getByRole('button', { name: 'I recorded it — hide and verify', exact: true }).click();
    const input = page.getByRole('textbox', { name: 'Re-enter all recovery words', exact: true });
    const confirm = page.getByRole('button', { name: 'Confirm and save wallet', exact: true });
    const invalid = numberedFixture.replace('about', 'abandon');
    await input.fill(invalid);
    await confirm.click();
    await expect(page.getByRole('alert')).toContainText('checksum');
    await expect(input).toHaveValue(invalid);
    expect((await bridge({ request, command: 'snapshot' })).roots).toHaveLength(0);
    await input.fill(numberedFixture.replaceAll('\n', ' '));
    await confirm.click();
    await expect(page.getByRole('alert')).toContainText('one numbered word per line');
    await expect(input).toHaveValue(numberedFixture.replaceAll('\n', ' '));
    await input.fill(publicWords.map((word, index) => `${index + 1}. ${word.slice(0, 4)}`).join('\n'));
    await confirm.click();
    await expect(input).toHaveCount(0);
    expect((await bridge({ request, command: 'snapshot' })).roots).toHaveLength(1);
    await page.getByRole('button', { name: 'Wallets & recovery', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Public recovery fixture', exact: true })).toBeVisible();
});
test('import and verification retain typos, support spaces, lines and four-letter prefixes', async ({ page, request }) => {
    await create({ page });
    await page.getByRole('button', { name: 'Create or import', exact: true }).click();
    await page.getByRole('button', { name: 'Import recovery phrase', exact: true }).click();
    await page.getByRole('textbox', { name: 'Wallet label', exact: true }).fill('Imported fixture');
    const input = page.getByRole('textbox', { name: 'Recovery phrase', exact: true });
    const invalid = publicPrefixes.replace('abou', 'typo');
    await input.fill(invalid);
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(input).toHaveValue(invalid);
    await input.fill(publicPrefixes);
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(input).toHaveValue('');
    await page.getByRole('button', { name: 'Wallets & recovery', exact: true }).click();
    const snapshot = await bridge({ request, command: 'snapshot' });
    await page.getByRole('combobox', { name: 'Root wallet', exact: true }).selectOption(snapshot.roots[0].id);
    await input.fill(numberedFixture.replace('about', 'abandon'));
    await page.getByRole('button', { name: 'Verify recovery', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('checksum');
    await expect(input).toHaveValue(numberedFixture.replace('about', 'abandon'));
    await input.fill(publicPrefixes.replaceAll(' ', '\n'));
    await page.getByRole('button', { name: 'Verify recovery', exact: true }).click();
    await expect(input).toHaveValue('');
    await expect(page.getByRole('status').filter({ hasText: 'Recovery phrase matches.' })).toBeVisible();
});
test('wallet pages stay separate and passphrases add an identity without renaming Default', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.getByRole('button', { name: 'Wallets & recovery', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Generate recovery phrase', exact: true })).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'Add passphrase identity', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Passphrases', exact: true }).click();
    const snapshot = await bridge({ request, command: 'snapshot' });
    await page.getByRole('combobox', { name: 'Root wallet', exact: true }).selectOption(snapshot.roots[0].id);
    await page.getByRole('textbox', { name: 'Label', exact: true }).fill('Public extra identity');
    await page.getByLabel('BIP39 passphrase', { exact: true }).fill('PUBLIC TEST ONLY');
    await page.getByRole('button', { name: 'Add passphrase identity', exact: true }).click();
    await expect(page.getByLabel('BIP39 passphrase', { exact: true })).toHaveValue('');
    const updated = await bridge({ request, command: 'snapshot' });
    expect(updated.identities.map((identity: { label: string }) => identity.label)).toEqual(['Default', 'Public extra identity']);
    await expect(page.getByRole('button', { name: 'Verify recovery', exact: true })).toHaveCount(0);
});
test('help sits beside headings, dismisses on backdrop and links directly to instruction topics', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    for (const title of [
        'Overview',
        'Create or import',
        'Wallets & recovery',
        'Passphrases',
        'Accounts',
        'Signing requests',
        'Activity',
        'Cryptocurrencies',
        'Settings',
        'Instructions',
    ]) {
        await page.locator('aside nav').getByRole('button', { name: title, exact: true }).click();
        const help = page.locator('.page-title .info-button');
        await expect(help).toHaveCount(1);
        await help.click();
        const dialog = page.getByRole('dialog', { name: title, exact: true });
        await expect(dialog).toBeVisible();
        await dialog.getByRole('heading', { name: title, exact: true }).click();
        await expect(dialog).toBeVisible();
        await page.mouse.click(5, 5);
        await expect(dialog).not.toBeVisible();
    }
    await page.getByRole('button', { name: 'Create or import', exact: true }).click();
    const rootLabel = page.locator('label[for="root-key-password"]');
    const rootInfo = page.getByRole('button', { name: 'Custom root password (optional) — More information', exact: true });
    const [labelBox, infoBox] = await Promise.all([rootLabel.boundingBox(), rootInfo.boundingBox()]);
    expect(labelBox && infoBox && Math.abs(labelBox.y + labelBox.height / 2 - infoBox.y - infoBox.height / 2)).toBeLessThan(2);
    await rootInfo.click();
    await page.getByRole('dialog').getByRole('button', { name: 'Learn more', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Instructions', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Passwords & backups', exact: true })).toBeVisible();
    expect((await page.locator('.wiki-article').boundingBox())?.width).toBeGreaterThan(400);
    await page.getByRole('button', { name: 'Advanced instructions', exact: true }).click();
    await expect(page.getByText(/Vault encryption uses Argon2id/)).toBeVisible();
    await page.getByRole('button', { name: 'Accounts', exact: true }).click();
    await page.locator('.page-title .info-button').click();
    await expect(page.getByRole('dialog')).toContainText('A wallet holds the recovery secret');
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Signing requests', exact: true }).click();
    await expect(page.locator('header').getByRole('button', { name: 'Import file', exact: true })).toBeVisible();
    await expect(page.locator('section.empty').getByRole('button', { name: 'Import file', exact: true })).toBeVisible();
});
test('account deletion asks first, persists, and leaves its wallet available for rederivation', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.getByRole('button', { name: 'Accounts', exact: true }).click();
    const original = await bridge({ request, command: 'snapshot' });
    await page.getByRole('button', { name: 'Delete account', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Delete this account?', exact: true });
    await expect(dialog).toContainText(original.accounts[0].address);
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect((await bridge({ request, command: 'snapshot' })).accounts).toHaveLength(1);
    await page.getByRole('button', { name: 'Delete account', exact: true }).click();
    await dialog.getByRole('button', { name: 'Yes, delete account', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Public testnet account', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Lock vault', exact: true }).click();
    await page.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await page.getByRole('button', { name: 'Unlock vault', exact: true }).click();
    await page.getByRole('button', { name: 'Accounts', exact: true }).click();
    const snapshot = await bridge({ request, command: 'snapshot' });
    expect(snapshot.accounts).toHaveLength(0);
    expect(snapshot.roots).toHaveLength(1);
    expect(snapshot.identities).toHaveLength(1);
    await page.getByRole('combobox', { name: 'Signing identity', exact: true }).selectOption(snapshot.identities[0].id);
    await page.getByRole('combobox', { name: 'Network', exact: true }).selectOption('testnet');
    await page.getByRole('textbox', { name: 'Label', exact: true }).fill('Recovered account');
    await page.getByRole('button', { name: 'Derive account', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Recovered account', exact: true })).toBeVisible();
    expect((await bridge({ request, command: 'snapshot' })).accounts[0].address).toBe(original.accounts[0].address);
});
