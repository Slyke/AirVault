import { test, expect, type APIRequestContext } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { helpTopics, relatedTopics } from '../../src/lib/help';
import { readFileSync } from 'node:fs';
const l: Record<string, string> = Object.fromEntries(
    Object.entries(JSON.parse(readFileSync(new URL('../../src/lib/en-US.json', import.meta.url), 'utf8'))).map(([key, value]) => [
        key.replace('airvault-', ''),
        String(value),
    ]),
);

const bridge = async ({ request, command, args = {} }: { request: APIRequestContext; command: string; args?: object }) => {
    const response = await request.post('/__airvault_test', { data: { command, args } });
    const body = await response.json();
    expect(body.ok).toBe(true);
    return body.value;
};
test.beforeEach(async ({ page, request }) => {
    await bridge({ request, command: 'test_reset' });
    await page.goto('/tests/ui/index.html');
});
test('mode explanation includes all roles, the Proxy restriction and the correct guide', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(page.locator('#settings-mode option[value="proxy"]')).toBeDisabled();
    await page.getByRole('button', { name: 'Operating mode — More information', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Operating mode', exact: true });
    for (const title of ['Offline', 'Online', 'Proxy']) await expect(dialog.locator('strong', { hasText: title })).toBeVisible();
    await expect(dialog).toContainText('this vault contains private keys');
    await dialog.getByRole('button', { name: 'Learn more', exact: true }).click();
    await expect(page.locator('.wiki-article h2').first()).toHaveText('Operating modes');
    await expect(page.locator('.wiki-article')).toContainText('fresh vault');
});
test('never timeout persists through a manual lock and unlock', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Settings', exact: true }).click();
    const security = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Security', exact: true }) });
    await security.getByRole('spinbutton', { name: 'Auto-lock after inactivity (seconds)', exact: true }).fill('-1');
    await expect(security).toContainText('Closing AirVault still locks');
    await security.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await security.getByLabel('I confirm these security and mode changes.', { exact: true }).check();
    await security.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(async () => (await bridge({ request, command: 'snapshot' })).settings.timeoutSeconds).toBe(-1);
    await expect(page.locator('.working')).toHaveCount(0);
    await page.getByRole('button', { name: 'Lock vault', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Unlock vault', exact: true })).toBeVisible();
    await page.getByLabel('Vault password', { exact: true }).fill('public-test-password');
    await page.getByRole('button', { name: 'Unlock vault', exact: true }).click();
    await page.locator('aside nav').getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(page.getByRole('spinbutton', { name: 'Auto-lock after inactivity (seconds)', exact: true })).toHaveValue('-1');
    await page.getByRole('button', { name: 'Auto-lock after inactivity (seconds) — More information', exact: true }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Learn more', exact: true }).click();
    await expect(page.locator('.wiki-article h2').first()).toHaveText('Locking your vault');
});
test('entropy help is inline, explains every source and opens its comparison guide', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Create or import', exact: true }).click();
    const label = page.locator('label[for="entropy-source"]');
    const info = page.getByRole('button', { name: 'Entropy source — More information', exact: true });
    const a = await label.boundingBox(),
        b = await info.boundingBox();
    expect(Math.abs(a!.y + a!.height / 2 - b!.y - b!.height / 2)).toBeLessThan(2);
    await info.click();
    const dialog = page.getByRole('dialog', { name: 'Entropy source', exact: true });
    for (const text of ['OS randomness:', 'Manual prefix + OS randomness:', 'Full entropy:', 'OS adds no randomness'])
        await expect(dialog).toContainText(text);
    await dialog.getByRole('button', { name: 'Learn more', exact: true }).click();
    await expect(page.locator('.wiki-article h2').first()).toHaveText('Choosing an entropy source');
    await page.getByRole('button', { name: 'Advanced instructions', exact: true }).click();
    await expect(page.locator('.wiki-article')).toContainText('7 OS bits');
});
test('all wiki articles, levels, related links, search and offline PDF export work', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Instructions', exact: true }).click();
    for (const topic of helpTopics) {
        await page
            .locator('.wiki-topics')
            .getByRole('button', { name: l['topic_' + topic], exact: true })
            .click();
        await expect(page.locator('#wiki-article-title')).toHaveText(l['topic_' + topic]);
        for (const level of ['basic', 'advanced']) {
            await page.getByRole('button', { name: l[level + 'Instructions'], exact: true }).click();
            expect((await page.locator('.wiki-article > p').allTextContents()).join(' ').length).toBeGreaterThan(100);
        }
        for (const next of relatedTopics[topic]) {
            expect(helpTopics).toContain(next);
            await expect(
                page.locator('.wiki-related').getByRole('button', { name: l['topic_' + next], exact: true }),
            ).toBeVisible();
        }
    }
    await page.getByLabel('Search instructions', { exact: true }).fill('ENT − 11n');
    await expect(page.locator('.wiki-topics button')).toHaveCount(1);
    await page.locator('.wiki-topics button').click();
    await expect(page.locator('#wiki-article-title')).toHaveText('Choosing an entropy source');
    await expect(page.getByLabel('Search instructions', { exact: true })).toHaveValue('');
    await page.locator('.wiki-related').getByRole('button', { name: 'Physical dice', exact: true }).click();
    await page.getByRole('button', { name: 'Previous article', exact: true }).click();
    await expect(page.locator('#wiki-article-title')).toHaveText('Choosing an entropy source');
    await page.locator('.wiki-topics').getByRole('button', { name: 'Bitcoin whitepaper', exact: true }).click();
    await page.getByRole('button', { name: 'Save Bitcoin whitepaper PDF', exact: true }).click();
    await expect(page.getByRole('status')).toContainText('Bitcoin whitepaper saved');
    expect(await bridge({ request, command: 'test_saved_whitepaper' })).toBe(
        'b1674191a88ec5cdd733e4240a81803105dc412d6c6708d53ab94fc248f4f553',
    );
    const pdf = await request.get('/reference/bitcoin-whitepaper.pdf');
    expect(
        createHash('sha256')
            .update(await pdf.body())
            .digest('hex'),
    ).toBe('b1674191a88ec5cdd733e4240a81803105dc412d6c6708d53ab94fc248f4f553');
});
test('About works while locked and compares the actual native executable checksum', async ({ page, request, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.getByRole('button', { name: 'About', exact: true }).click();
    const about = page.getByRole('dialog', { name: 'About', exact: true });
    const info = await bridge({ request, command: 'app_info' });
    const independentlyHashed = createHash('sha256')
        .update(await readFile(info.binaryPath))
        .digest('hex');
    await expect(about.getByTestId('binary-hash')).toHaveText(independentlyHashed);
    await about.getByLabel('Compare with a trusted SHA-256').fill(independentlyHashed.toUpperCase());
    await expect(about.getByRole('status')).toHaveText('The file matches this checksum.');
    await about.getByLabel('Compare with a trusted SHA-256').fill('0'.repeat(64));
    await expect(about.getByRole('status')).toHaveText('The file does not match this checksum.');
    await about.getByLabel('Compare with a trusted SHA-256').fill('invalid');
    await expect(about.getByRole('status')).toContainText('exactly 64 hexadecimal');
    await about.getByRole('button', { name: 'Copy binary hash', exact: true }).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(independentlyHashed);
    await about.getByRole('button', { name: 'Recalculate hash', exact: true }).click();
    await expect(about.getByRole('button', { name: 'Recalculate hash', exact: true })).toBeEnabled();
    await expect(about.getByTestId('binary-hash')).toHaveText(independentlyHashed);
    await about.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(about).not.toBeVisible();
    expect((await bridge({ request, command: 'snapshot' })).unlocked).toBe(false);
});
test('dice alternatives and recorded-word review have visible separation', async ({ page, request }) => {
    await bridge({ request, command: 'test_seed' });
    await page.reload();
    await page.locator('aside nav').getByRole('button', { name: 'Create or import', exact: true }).click();
    for (let i = 0; i < 11; i++) await page.getByRole('button', { name: 'Roll 1', exact: true }).click();
    const accept = await page.getByRole('button', { name: 'Add this word', exact: true }).boundingBox();
    const alternative = await page.locator('.dice-alternative details').boundingBox();
    expect(alternative!.y - accept!.y - accept!.height).toBeGreaterThanOrEqual(40);
    await page.getByRole('button', { name: 'Add this word', exact: true }).click();
    const generate = await page.locator('.generation-action').boundingBox();
    const review = await page.getByRole('heading', { name: 'Review your recorded words', exact: true }).boundingBox();
    expect(review!.y - generate!.y - generate!.height).toBeGreaterThanOrEqual(50);
});
