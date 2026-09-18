// 一次性诊断脚本：反复导航，观察连接数是否随导航累积。
// 用法：node scripts/diagnose-connections.mjs
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { tmpdir } from 'node:os';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const BASE = process.env.UI_BASE ?? 'http://127.0.0.1:8080';
const USER = process.env.UI_USER ?? 'admin';
const PASS = process.env.UI_PASS ?? 'admin12345';
const PORT = 9444;
const ROUNDS = Number(process.env.ROUNDS ?? 6);

function countConnections() {
  return new Promise((resolve) => {
    const child = spawn('sh', ['-c', "lsof -nP -iTCP:8080 2>/dev/null | grep -c ESTABLISHED"]);
    let out = '';
    child.stdout.on('data', (chunk) => (out += chunk));
    child.on('close', () => resolve(Number(out.trim()) || 0));
  });
}

// 必须用独立配置目录：跑在默认配置上会抢走用户在用的配置锁，
// 退出时把用户已经打开的浏览器一起带崩。
const chrome = spawn(CHROME, [
  '--headless=new',
  '--disable-gpu',
  '--no-sandbox',
  `--user-data-dir=${tmpdir()}/circulation-diag-profile`,
  '--no-first-run',
  '--no-default-browser-check',
  `--remote-debugging-port=${PORT}`,
  '--window-size=1280,900',
  'about:blank',
], { stdio: 'ignore' });

console.log(`诊断浏览器已启动（PID ${chrome.pid}），使用独立配置，不影响你已打开的浏览器\n`);

let target = null;
for (let i = 0; i < 60; i += 1) {
  try {
    const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
    target = list.find((item) => item.type === 'page');
    if (target) break;
  } catch {
    // 还没起来
  }
  await delay(250);
}
if (!target) throw new Error('Chrome 未就绪');

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const entry = pending.get(message.id);
  if (!entry) return;
  pending.delete(message.id);
  entry.resolve(message.result);
};

function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve) => {
    pending.set(id, { resolve });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) return `异常：${result.exceptionDetails.text}`;
  return result.result.value;
}

await send('Page.enable');
await send('Runtime.enable');

await send('Page.navigate', { url: `${BASE}/login` });
await delay(1200);
await evaluate(`
  (async () => {
    await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({ username: ${JSON.stringify(USER)}, password: ${JSON.stringify(PASS)} }),
    });
    return true;
  })()
`);

console.log('轮次\t连接数\t页面能否发请求');

for (let round = 1; round <= ROUNDS; round += 1) {
  // 交替登录不同账号，模拟走查脚本里的身份切换
  const who = round % 2 === 0 ? ['probe1', 'probe123456'] : [USER, PASS];
  await evaluate(`
    (async () => {
      await fetch('/api/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ username: ${JSON.stringify(who[0])}, password: ${JSON.stringify(who[1])} }),
      });
      return true;
    })()
  `);
  await send('Page.navigate', { url: `${BASE}/dashboard` });
  await delay(1800);

  const connections = await countConnections();
  const probe = await evaluate(`
    (async () => {
      const started = Date.now();
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 4000);
      try {
        const response = await fetch('/api/health', { credentials: 'same-origin', signal: controller.signal });
        clearTimeout(timer);
        return response.status + '（' + (Date.now() - started) + 'ms）';
      } catch {
        clearTimeout(timer);
        return '超时（' + (Date.now() - started) + 'ms）';
      }
    })()
  `);

  console.log(`${round}\t${connections}\t${probe}`);
}

socket.close();
// 只结束自己拉起的进程，不做按名字的批量清理
try {
  process.kill(chrome.pid, 'SIGKILL');
} catch {
  /* 已退出 */
}
