// Separate test entry. Production index.html never imports this file.
import { mockIPC } from '@tauri-apps/api/mocks';
import { mount } from 'svelte';
import App from '../../src/App.svelte';
import '../../src/style.css';
import '../../src/themes.css';
import { applyAppearance, readAppearance } from '../../src/lib/appearance';
const marker = '__AIRVAULT_TEST_BRIDGE__';
mockIPC(async (command, args) => {
    const response = await fetch('/__airvault_test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command, args, marker }),
    });
    const body = await response.json();
    if (!body.ok) throw body.error;
    return body.value;
});
applyAppearance({ appearance: readAppearance() });
mount(App, { target: document.getElementById('app')! });
