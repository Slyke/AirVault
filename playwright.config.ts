import { defineConfig } from '@playwright/test';
export default defineConfig({
    testDir: './tests/ui',
    testMatch: '**/*.spec.ts',
    workers: 1,
    timeout: 60_000,
    use: { baseURL: 'http://127.0.0.1:1421', viewport: { width: 1280, height: 940 }, trace: 'off', screenshot: 'off' },
    webServer: {
        command: 'npx vite --config tests/ui/vite.config.ts',
        url: 'http://127.0.0.1:1421/tests/ui/index.html',
        reuseExistingServer: false,
    },
});
