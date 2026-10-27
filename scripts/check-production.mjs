import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
const forbidden = ['__AIRVAULT_TEST_BRIDGE__', '__airvault_test', 'test_fund', 'test_inspector_file', 'test_seed', 'test_multisig_seed', 'test_saved_multisig', 'test_saved_psbt', 'test_saved_transaction', 'mockIPC', 'ui_test_bridge'];
const walk = async ({ directory }) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) { await walk({ directory: path }); continue; }
        const bytes = await readFile(path);
        for (const marker of forbidden) if (bytes.includes(marker)) throw new Error('Test hook found in production artifact: ' + path);
    }
};
await walk({ directory: 'dist' });
for (const path of process.argv.slice(2)) {
    const bytes = await readFile(path);
    for (const marker of forbidden) if (bytes.includes(marker)) throw new Error('Test hook found in binary: ' + path);
}
console.log('Production artifacts contain no test bridge or UI automation hooks.');
