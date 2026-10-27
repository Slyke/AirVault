import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
export default defineConfig({
    plugins: [
        svelte(),
        {
            name: 'isolated-native-test-bridge',
            configureServer: (server) => {
                const process = spawn('./target/debug/examples/ui_test_bridge', [], { stdio: ['pipe', 'pipe', 'inherit'] });
                const queue: ((value: string) => void)[] = [];
                createInterface({ input: process.stdout }).on('line', (line) => queue.shift()?.(line));
                server.httpServer?.on('close', () => process.kill());
                server.middlewares.use('/__airvault_test', (request, response) => {
                    if (
                        request.method !== 'POST' ||
                        (request.headers.origin && request.headers.origin !== 'http://127.0.0.1:1421')
                    ) {
                        response.statusCode = 403;
                        response.end();
                        return;
                    }
                    let body = '';
                    request.on('data', (chunk) => {
                        body += chunk;
                        if (body.length > 2_100_000) request.destroy();
                    });
                    request.on('end', () => {
                        queue.push((value) => {
                            response.setHeader('Content-Type', 'application/json');
                            response.end(value);
                        });
                        process.stdin.write(body + '\n');
                    });
                });
            },
        },
    ],
    server: { host: '127.0.0.1', port: 1421, strictPort: true },
});
