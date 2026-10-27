import { test, expect, type APIRequestContext } from '@playwright/test';
import { createHash } from 'node:crypto';
const tx = '0200000001' + '00'.repeat(32) + '0000000000ffffffff01e803000000000000160014' + '11'.repeat(20) + '00000000';
const message = ({ damaged = false }: { damaged?: boolean } = {}) => {
    const payload = Buffer.from('2a00000000000000', 'hex');
    const checksum = createHash('sha256').update(createHash('sha256').update(payload).digest()).digest().subarray(0, 4);
    if (damaged) checksum[0] ^= 1;
    return Buffer.concat([
        Buffer.from('f9beb4d9', 'hex'),
        Buffer.from('ping\0\0\0\0\0\0\0\0'),
        Buffer.from('08000000', 'hex'),
        checksum,
        payload,
    ]).toString('hex');
};
const bridge = async ({ request, command, args = {} }: { request: APIRequestContext; command: string; args?: object }) => {
    const response = await request.post('/__airvault_test', { data: { command, args } });
    const body = await response.json();
    expect(body.ok, JSON.stringify(body.error)).toBe(true);
    return body.value;
};
test.beforeEach(async ({ request, page }) => {
    await bridge({ request, command: 'test_reset' });
    await bridge({ request, command: 'test_seed' });
    await page.goto('/tests/ui/index.html');
    page.on('pageerror', (error) => {
        throw error;
    });
});
test('hex and ASCII share hover, pin, keyboard selection and exact original-file hash', async ({ page }) => {
    await page.locator('aside nav').getByRole('button', { name: 'Payload inspector', exact: true }).click();
    await page.getByText('Paste hex, Base64 or an AirVault envelope', { exact: true }).click();
    await page.getByLabel('Payload text', { exact: true }).fill(tx);
    await page.getByRole('button', { name: 'Inspect text', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Bitcoin transaction', exact: true })).toBeVisible();
    await expect(page.locator('.inspector-properties').first()).toContainText(createHash('sha256').update(tx).digest('hex'));
    const hex = page.locator('.inspector-byte[data-pane="hex"][data-offset="0"]');
    const ascii = page.locator('.inspector-byte[data-pane="ascii"][data-offset="0"]');
    await hex.hover();
    await expect(page.locator('.inspector-helper h3')).toHaveText('Version');
    await expect(hex).toHaveClass(/inspector-byte-active/);
    await expect(ascii).toHaveClass(/inspector-byte-active/);
    await hex.click();
    await page.locator('.inspector-byte[data-pane="ascii"][data-offset="5"]').hover();
    await expect(page.locator('.inspector-helper h3')).toHaveText('Version');
    await page.getByRole('button', { name: 'Unpin', exact: true }).click();
    await ascii.focus();
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.inspector-byte[data-pane="ascii"][data-offset="16"]')).toBeFocused();
    await expect(page.locator('.inspector-helper h3')).toHaveText('Input 1 · previous txid');
    await page.getByLabel('Find a field', { exact: true }).fill('scriptPubKey');
    await page.locator('.inspector-field-list button').filter({ hasText: 'scriptPubKey' }).last().click();
    await expect(page.locator('.inspector-helper')).toContainText('Bitcoin Script');
    await page.getByRole('combobox', { name: 'Byte document', exact: true }).selectOption('0');
    await expect(page.getByRole('heading', { name: 'Text encoding', exact: true })).toBeVisible();
    await page.screenshot({ path: '.local/inspector-hex.png', fullPage: true });
});
test('any file extension, corrupt P2P checksum and unknown bytes remain inspectable', async ({ page, request }) => {
    const hex = message({ damaged: true });
    await bridge({ request, command: 'test_inspector_file', args: { hex } });
    await page.locator('aside nav').getByRole('button', { name: 'Payload inspector', exact: true }).click();
    await page.getByRole('button', { name: 'Open any file', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Bitcoin P2P v1', exact: true })).toBeVisible();
    await expect(page.getByRole('alert')).toContainText('Some protocol checks failed');
    await expect(page.locator('.inspector-fail')).toContainText('checksum');
    await expect(page.locator('.inspector-fail')).toContainText('calculated');
    await page.getByText('Paste hex, Base64 or an AirVault envelope', { exact: true }).click();
    await page.getByLabel('Payload text', { exact: true }).fill('Hello <script>arbitrary file</script>');
    await page.getByRole('button', { name: 'Inspect text', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Unrecognized data', exact: true })).toBeVisible();
    await expect(page.locator('.inspector-byte[data-pane="ascii"][data-offset="0"]')).toHaveText('H');
    await expect(page.locator('.payload-inspector script')).toHaveCount(0);
    await page.screenshot({ path: '.local/inspector-p2p.png', fullPage: true });
});
test('review inspector is read-only and exposes the current PSBT and linked instructions', async ({ page, request }) => {
    const id = await bridge({ request, command: 'test_fund' });
    const before = await bridge({ request, command: 'snapshot' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Signing requests', exact: true }).click();
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    await page.locator('dialog[open] summary').filter({ hasText: 'Inspect payload' }).click();
    const review = page.locator('dialog').filter({ has: page.getByRole('heading', { name: /Bitcoin PSBT/ }) });
    await expect(review.getByRole('combobox', { name: 'Byte document', exact: true })).toBeVisible();
    await review.getByRole('combobox', { name: 'Signing payload source', exact: true }).selectOption('psbt');
    await expect(review.getByRole('heading', { name: 'Bitcoin PSBT', exact: true })).toBeVisible();
    const after = await bridge({ request, command: 'snapshot' });
    expect(after.pending).toEqual(before.pending);
    expect(after.responses).toEqual(before.responses);
    expect(after.history).toEqual(before.history);
    await page.getByRole('button', { name: 'Close review', exact: true }).click();
    await page.getByRole('button', { name: 'Inspect payload', exact: true }).click();
    await expect(page.getByRole('combobox', { name: 'Signing payload source', exact: true })).toBeVisible();
    const direct = await bridge({ request, command: 'inspect_exchange', args: { id, kind: 'request' } });
    expect(
        direct.documents[0].checks.some(
            (c: { name: string; status: string }) => c.name === 'Payload SHA-256' && c.status === 'pass',
        ),
    ).toBe(true);
    await page.locator('.page-title').getByRole('button', { name: 'Payload inspector — More information', exact: true }).click();
    await page
        .getByRole('dialog', { name: 'Payload inspector', exact: true })
        .getByRole('button', { name: 'Learn more', exact: true })
        .click();
    await expect(page.locator('#wiki-article-title')).toHaveText('Inspecting payload bytes');
});
test('inspector stays available and cannot change approval or wallet state in all three modes', async ({ page, request }) => {
    for (const mode of ['offline', 'combined', 'proxy']) {
        await bridge({ request, command: 'test_reset' });
        await bridge({ request, command: 'create_vault', args: { password: 'public-test-password', mode } });
        await page.reload();
        await page.locator('aside nav').getByRole('button', { name: 'Payload inspector', exact: true }).click();
        await page.getByText('Paste hex, Base64 or an AirVault envelope', { exact: true }).click();
        await page.getByLabel('Payload text', { exact: true }).fill(message());
        await page.getByRole('button', { name: 'Inspect text', exact: true }).click();
        await expect(page.getByRole('heading', { name: 'Bitcoin P2P v1', exact: true })).toBeVisible();
        await expect(page.locator('.inspector-checks')).toContainText('Passed');
        const state = await bridge({ request, command: 'snapshot' });
        expect(state.settings.mode).toBe(mode);
        expect(state.pending).toEqual([]);
        expect(state.responses).toEqual([]);
        expect(state.accounts).toEqual([]);
    }
});
