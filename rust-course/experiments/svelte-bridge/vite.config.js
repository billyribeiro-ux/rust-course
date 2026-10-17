import { defineConfig } from 'vite';
import { Agent } from 'node:http';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Local teaching proxy: one read-only route, at most four upstream requests.
// A Vite development server is not a reviewed production deployment.
export function snapshotAdmission() {
  return {
    name: 'local-snapshot-admission',
    configureServer(server) {
      let active = 0;
      server.middlewares.use((request, response, next) => {
        if (!request.url?.startsWith('/api/')) return next();
        if (request.url !== '/api/snapshot') { response.writeHead(404).end(); return; }
        if (request.method !== 'GET') { response.writeHead(405, { Allow: 'GET' }).end(); return; }
        if (request.headers['transfer-encoding'] || Number(request.headers['content-length'] ?? 0) !== 0) { response.writeHead(400).end(); return; }
        if (request.headers.origin && request.headers.origin !== 'http://127.0.0.1:4176') { response.writeHead(403).end(); return; }
        if (active >= 4) { response.writeHead(429).end(); return; }
        active += 1;
        let released = false;
        const release = () => { if (!released) { released = true; active -= 1; } };
        response.once('close', release);
        response.once('finish', release);
        next();
      });
    }
  };
}
export default defineConfig({
  plugins: [snapshotAdmission(), svelte()],
  server: {
    host: '127.0.0.1', port: 4176, strictPort: true,
    proxy: { '^/api/snapshot$': { target: 'http://127.0.0.1:4116', timeout: 3000, proxyTimeout: 3000,
      agent: new Agent({ keepAlive: false, maxSockets: 4, maxTotalSockets: 4 }),
      configure(proxy) {
        proxy.on('proxyReq', (outbound, _request, response) => {
          if (response.destroyed) outbound.destroy();
          else response.once('close', () => outbound.destroy());
        });
      }
    } }
  }
});
