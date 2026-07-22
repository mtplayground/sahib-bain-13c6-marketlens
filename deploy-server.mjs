import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const distDir = path.join(__dirname, 'frontend', 'dist');
const backendOrigin = process.env.BACKEND_ORIGIN || 'http://127.0.0.1:8081';
const port = Number(process.env.PUBLIC_PORT || 8080);

const mime = new Map([
  ['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'], ['.json', 'application/json; charset=utf-8'],
  ['.svg', 'image/svg+xml'], ['.png', 'image/png'], ['.jpg', 'image/jpeg'],
  ['.jpeg', 'image/jpeg'], ['.ico', 'image/x-icon'], ['.woff2', 'font/woff2']
]);

function proxy(req, res) {
  const target = new URL(req.url, backendOrigin);
  const headers = { ...req.headers, host: target.host, 'x-forwarded-host': req.headers.host || '', 'x-forwarded-proto': 'https' };
  const upstream = http.request(target, { method: req.method, headers }, (upstreamRes) => {
    res.writeHead(upstreamRes.statusCode || 502, upstreamRes.headers);
    upstreamRes.pipe(res);
  });
  upstream.on('error', (err) => {
    console.error('[proxy error]', { name: err.name, code: err.code, message: err.message, stack: err.stack?.split('\n').slice(0, 5).join('\n') });
    res.writeHead(502, { 'content-type': 'text/plain; charset=utf-8' });
    res.end('Bad Gateway');
  });
  req.pipe(upstream);
}

const server = http.createServer((req, res) => {
  if (!req.url) return res.end();
  const url = new URL(req.url, 'http://localhost');
  if (url.pathname.startsWith('/api/') || url.pathname === '/ws') return proxy(req, res);
  const pathname = decodeURIComponent(url.pathname);
  let filePath = path.join(distDir, pathname === '/' ? 'index.html' : pathname);
  if (!filePath.startsWith(distDir)) {
    res.writeHead(403); return res.end('Forbidden');
  }
  fs.stat(filePath, (err, stat) => {
    if (err || !stat.isFile()) filePath = path.join(distDir, 'index.html');
    fs.readFile(filePath, (readErr, body) => {
      if (readErr) {
        console.error('[static error]', { name: readErr.name, code: readErr.code, message: readErr.message, stack: readErr.stack?.split('\n').slice(0, 5).join('\n') });
        res.writeHead(500, { 'content-type': 'text/plain; charset=utf-8' }); return res.end('Server error');
      }
      res.writeHead(200, { 'content-type': mime.get(path.extname(filePath)) || 'application/octet-stream' });
      res.end(body);
    });
  });
});

server.on('upgrade', (req, socket) => {
  const target = new URL(req.url || '/ws', backendOrigin.replace('http:', 'ws:'));
  const upstream = http.request(target, { method: 'GET', headers: { ...req.headers, host: target.host } });
  upstream.on('upgrade', (res, upstreamSocket, head) => {
    socket.write(`HTTP/1.1 ${res.statusCode} ${res.statusMessage}\r\n` + Object.entries(res.headers).map(([k,v]) => `${k}: ${v}`).join('\r\n') + '\r\n\r\n`);
    if (head.length) upstreamSocket.unshift(head);
    upstreamSocket.pipe(socket); socket.pipe(upstreamSocket);
  });
  upstream.on('error', () => socket.destroy());
  upstream.end();
});

server.listen(port, '0.0.0.0', () => console.log(`serving frontend on ${port}, proxying API to ${backendOrigin}`));
