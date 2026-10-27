import { test, expect, type APIRequestContext } from '@playwright/test';
const bridge = async (request: APIRequestContext, command: string, args: object = {}) => {
    const response = await request.post('/__airvault_test', { data: { command, args } });
    const body = await response.json();
    expect(body.ok, JSON.stringify(body.error)).toBe(true);
    return body.value;
};
test('multisig setup, backup, partial signing and linked wiki', async ({ page, request }) => {
    page.on('pageerror', (error) => {
        throw error;
    });
    await bridge(request, 'test_reset');
    const fixture = await bridge(request, 'test_multisig_seed');
    await page.goto('/tests/ui/index.html');
    await page.getByRole('button', { name: 'Bitcoin multisig', exact: true }).click();
    await page.getByRole('combobox', { name: 'Network', exact: true }).selectOption('regtest');
    await page.getByRole('combobox', { name: 'Signing identity', exact: true }).selectOption(fixture.identity);
    await page.getByRole('button', { name: 'Show public cosigner key' }).click();
    await expect(page.getByLabel('Public cosigner key', { exact: true })).toHaveValue(fixture.cosigners[0]);
    await page.getByLabel('Wallet label', { exact: true }).fill('2-of-3 test wallet');
    await page.getByLabel('Cosigner keys — one per line').fill(fixture.cosigners.join('\n'));
    await page.getByRole('button', { name: 'Review configuration' }).click();
    await expect(page.getByRole('heading', { name: '2-of-3 test wallet · 2 / 3 · regtest' })).toBeVisible();
    const register = page.getByRole('button', { name: 'Register multisig account' });
    await expect(register).toBeDisabled();
    await page.getByLabel('I checked the threshold, network, address and every cosigner against the other devices.').check();
    await register.click();
    await page.getByRole('button', { name: 'Save configuration backup' }).click();
    await expect(page.getByRole('status')).toHaveText('File exported.');
    const backup = await bridge(request, 'test_saved_multisig');
    expect(backup.policy.cosigners).toHaveLength(3);
    expect(backup.receiveDescriptor).toMatch(/^wsh\(sortedmulti\(2,.*\)\)#[a-z0-9]{8}$/);
    await page.getByRole('button', { name: 'Load configuration' }).click();
    await expect(page.getByRole('heading', { name: '2-of-3 test wallet · 2 / 3 · regtest' })).toBeVisible();
    await page.getByRole('button', { name: 'Edit configuration' }).click();
    await page.getByRole('button', { name: 'Prepare next address' }).click();
    await expect(page.getByLabel('Address index', { exact: true })).toHaveValue('1');
    await page.screenshot({ path: '.local/multisig-setup.png', fullPage: true });
    await bridge(request, 'test_fund');
    await page.reload();
    await page.getByRole('button', { name: 'Signing requests', exact: true }).click();
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    await expect(page.getByRole('heading', { name: '0 / 2 required signatures' })).toBeVisible();
    await page.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await page.getByRole('checkbox').check();
    await page.getByRole('button', { name: 'Approve & sign', exact: true }).click();
    await page.getByRole('button', { name: 'Export PSBT', exact: true }).click();
    await expect(page.getByRole('status')).toHaveText('File exported.');
    expect(await bridge(request, 'test_saved_psbt')).toEqual([1]);
    await page.getByRole('button', { name: 'Review exact transaction', exact: true }).click();
    await expect(page.getByRole('heading', { name: '1 / 2 required signatures' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Broadcast', exact: true })).toHaveCount(0);
    await page.screenshot({ path: '.local/multisig-signing.png' });
    await page.getByRole('button', { name: 'Close review', exact: true }).click();
    await page.getByRole('button', { name: 'Instructions', exact: true }).click();
    for (const topic of [
        'Set up Bitcoin multisig',
        'Collect multisig signatures',
        'Multisig backups & recovery',
        'Multisig risks & key loss',
    ]) {
        await page
            .getByRole('navigation', { name: 'Instruction topics' })
            .getByRole('button', { name: topic, exact: true })
            .click();
        await expect(page.locator('#wiki-article-title')).toHaveText(topic);
        await page.getByRole('button', { name: 'Advanced instructions' }).click();
        await expect(page.locator('.wiki-article')).not.toContainText('undefined');
        await page.getByRole('button', { name: 'Basic instructions' }).click();
    }
});
