// 用 Chrome DevTools Protocol 驱动无头浏览器走一遍真实登录流程并逐页截图，
// 用于本地验收界面。前端的 React 受控输入需要按原生 setter 赋值，直接改 value 不会触发更新。
//
//   node scripts/ui-shot.mjs
//
// 环境变量：UI_BASE / UI_OUT / UI_USER / UI_PASS

import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import { tmpdir } from 'node:os';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const BASE = process.env.UI_BASE ?? 'http://127.0.0.1:8080';
const OUT = process.env.UI_OUT ?? '/tmp/ui-shots';
const USER = process.env.UI_USER ?? 'admin';
const PASS = process.env.UI_PASS ?? 'admin123';
const NEW_PASS = process.env.UI_NEW_PASS ?? 'admin12345';
const DEBUG_PORT = 9333;

const PAGES = [
  { path: '/dashboard', name: '04-dashboard' },
  { path: '/admin/fields', name: '05-fields' },
  { path: '/admin/users', name: '06-users' },
  { path: '/admin/roles', name: '07-roles' },
];

mkdirSync(OUT, { recursive: true });

// 必须用独立的配置目录。
//
// 早期版本直接用 Chrome 的默认配置跑无头截图，会抢走正在使用的配置文件的锁，
// 退出时把用户已经打开的浏览器一起带崩。这里固定用临时目录，并断言参数在位。
const PROFILE_DIR = `${tmpdir()}/circulation-ui-shot-profile`;

const chrome = spawn(
  CHROME,
  [
    '--headless=new',
    '--disable-gpu',
    '--no-sandbox',
    '--hide-scrollbars',
    `--user-data-dir=${PROFILE_DIR}`,
    '--no-first-run',
    '--no-default-browser-check',
    `--remote-debugging-port=${DEBUG_PORT}`,
    '--window-size=1440,1100',
    'about:blank',
  ],
  { stdio: 'ignore' },
);

if (!chrome.pid) {
  throw new Error('浏览器没能启动');
}

console.log(`浏览器已启动（PID ${chrome.pid}）`);
console.log(`使用独立配置 ${PROFILE_DIR}，不会影响你已打开的浏览器\n`);

const cleanup = () => {
  try {
    // 只按 PID 精确结束自己拉起的这个进程，绝不做按名字的批量清理
    process.kill(chrome.pid, 'SIGKILL');
  } catch {
    /* 进程已退出 */
  }
};

process.on('exit', cleanup);
process.on('SIGINT', () => {
  cleanup();
  process.exit(130);
});

async function findTarget() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === 'page');
      if (page?.webSocketDebuggerUrl) return page;
    } catch {
      /* 还没起来 */
    }
    await delay(250);
  }
  throw new Error('Chrome 调试端口未就绪');
}

const target = await findTarget();
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = () => reject(new Error('无法连接调试端口'));
});

let nextId = 1;
const pending = new Map();

socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const entry = pending.get(message.id);
  if (!entry) return;
  pending.delete(message.id);
  if (message.error) {
    entry.reject(new Error(JSON.stringify(message.error)));
  } else {
    entry.resolve(message.result);
  }
};

function send(method, params = {}, timeoutMs = 20000) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    // 加超时：机器负载高时 CDP 调用可能一直不返回，
    // 没有超时的话整个走查会静默挂死，看不出卡在哪一步
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CDP 调用超时：${method}`));
    }, timeoutMs);

    pending.set(id, {
      resolve: (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      reject: (error) => {
        clearTimeout(timer);
        reject(error);
      },
    });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    const detail = result.exceptionDetails.exception?.description ?? result.exceptionDetails.text;
    throw new Error(`页面脚本报错: ${detail}`);
  }
  return result.result.value;
}

/** 顺带把浏览器控制台里的报错收集出来，白屏问题一眼可见 */
const consoleErrors = [];
socket.addEventListener('message', (event) => {
  const message = JSON.parse(event.data);
  if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
    const text = message.params.args.map((arg) => arg.value ?? arg.description).join(' ');
    // 只关心本应用的报错，忽略浏览器里其它来源的噪音
    if (text.includes(BASE)) consoleErrors.push(text);
  }
  if (message.method === 'Runtime.exceptionThrown') {
    consoleErrors.push(message.params.exceptionDetails.exception?.description ?? '未知异常');
  }
});

async function goto(path, settleMs = 1500) {
  await send('Page.navigate', { url: `${BASE}${path}` });
  await delay(settleMs);
}

async function shot(name) {
  const { data } = await send('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: true,
  });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(data, 'base64'));
  console.log(`  截图 ${name}.png`);
}

await send('Page.enable');
await send('Runtime.enable');

/** 按原生 setter 赋值，否则 React 受控组件收不到变更 */
function fillInputs(values) {
  return `
    (() => {
      const setValue = (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
        setter.call(el, value);
        el.dispatchEvent(new Event('input', { bubbles: true }));
      };
      const inputs = document.querySelectorAll('input');
      const values = ${JSON.stringify(values)};
      if (inputs.length < values.length) return 0;
      values.forEach((value, index) => setValue(inputs[index], value));
      return inputs.length;
    })()
  `;
}

async function submitForm() {
  await delay(300);
  await evaluate(`document.querySelector('button[type=submit]').click()`);
}

/** 按可见文本点按钮或链接，用来进入抽屉、弹窗这类二级界面 */
async function clickByText(text) {
  return evaluate(`
    (() => {
      // antd 会给两个汉字的按钮插入空格，比较前统一去掉空白
      const normalize = (value) => (value ?? '').replace(/\\s+/g, '');
      const wanted = normalize(${JSON.stringify(text)});
      const candidates = [...document.querySelectorAll('a, button')];
      const target =
        candidates.find((el) => normalize(el.textContent) === wanted) ??
        candidates.find((el) => normalize(el.textContent).includes(wanted));
      if (!target) return false;
      target.click();
      return true;
    })()
  `);
}

/** 弹窗底部的确定按钮：页面上往往还有同名按钮，必须限定在 footer 里找 */
async function clickModalOk() {
  return evaluate(`
    (() => {
      const button = document.querySelector('.ant-modal-footer .ant-btn-primary');
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
}

/** 展开 antd Select。它监听的是 mousedown，只 click 不会弹开。 */
async function openSelect(selectSelector = '.ant-modal .ant-select-content') {
  return evaluate(`
    (() => {
      const selector = document.querySelector(${JSON.stringify(selectSelector)});
      if (!selector) {
        const classes = [...document.querySelectorAll('.ant-modal *')]
          .map((el) => (typeof el.className === 'string' ? el.className : ''))
          .filter((name) => name.includes('select'));
        return 'no-selector:' + JSON.stringify([...new Set(classes)]);
      }
      for (const type of ['mousedown', 'mouseup', 'click']) {
        selector.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }));
      }
      return 'ok';
    })()
  `);
}

/** 在已展开的下拉里按文本选中一项；多选时下拉会保持展开，可以连续调用 */
async function clickSelectOption(label) {
  return evaluate(`
    (() => {
      const dropdowns = [...document.querySelectorAll('.ant-select-dropdown')].filter(
        (el) => !el.classList.contains('ant-select-dropdown-hidden'),
      );
      if (dropdowns.length === 0) return 'no-dropdown';

      // 不同 antd 版本的下拉项类名有差异，取最内层的候选项
      const dropdown = dropdowns[dropdowns.length - 1];
      const all = [...dropdown.querySelectorAll('[class*="ant-select-item"]')];
      const options = all.filter((el) => !all.some((other) => other !== el && el.contains(other)));
      if (options.length === 0) {
        return 'no-options:' + dropdown.className;
      }

      const option = options.find((el) => el.textContent.includes(${JSON.stringify(label)}));
      if (!option) {
        return 'no-match:' + options.map((el) => el.textContent).join('|');
      }
      for (const type of ['mousedown', 'mouseup', 'click']) {
        option.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }));
      }
      return 'ok';
    })()
  `);
}

/** 展开下拉并按姓名连选多人，返回诊断信息方便排查 */
async function pickAssignees(names) {
  const opened = await openSelect();
  if (opened !== 'ok') return opened;
  await delay(600);

  const results = [];
  for (const name of names) {
    results.push(`${name}:${await clickSelectOption(name)}`);
    await delay(300);
  }
  return results.join(' ');
}

/**
 * 等元素出现再点，点不到就抛错。
 *
 * 固定 sleep 再查一次很容易假失败——页面把接口都拉完才渲染出按钮，
 * 快一点慢一点都会踩到，所以这里改成轮询等待。
 */
let failureSeq = 0;
async function mustClick(text, failure, timeoutMs = 12000) {
  const deadline = Date.now() + timeoutMs;
  do {
    if (await clickByText(text)) return true;
    await delay(300);
  } while (Date.now() < deadline);

  // 失败时把当时页面上的按钮和地址打出来，省得再复现一次
  failureSeq += 1;
  await shot(`error-${failureSeq}`);
  const context = await evaluate(`
    (() => ({
      path: window.location.pathname,
      buttons: [...document.querySelectorAll('a, button')]
        .map((el) => el.textContent.replace(/\\s+/g, ''))
        .filter(Boolean)
        .slice(0, 40),
      body: document.body.innerText.replace(/\\s+/g, ' ').slice(0, 300),
    }))()
  `);
  console.log(`  页面路径 ${context.path}`);
  console.log(`  可见按钮 ${JSON.stringify(context.buttons)}`);
  console.log(`  正文 ${context.body}`);

  // 页面卡在加载中时，探一下同一个接口在页面里还能不能发出去
  const probe = await evaluate(`
    (async () => {
      const started = Date.now();
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 4000);
      try {
        const response = await fetch('/api/flows/instances/1', {
          credentials: 'same-origin',
          signal: controller.signal,
        });
        clearTimeout(timer);
        return 'ok ' + response.status + ' 用时 ' + (Date.now() - started) + 'ms';
      } catch (error) {
        clearTimeout(timer);
        return '失败 ' + error.name + ' 用时 ' + (Date.now() - started) + 'ms';
      }
    })()
  `);
  console.log(`  接口探针 ${probe}`);

  throw new Error(failure ?? `找不到可点击的「${text}」`);
}

/** 等图表真的画出柱子，避免截到还没渲染完的空白 */
async function waitForChart(timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const ready = await evaluate(
      `document.querySelectorAll('.recharts-bar-rectangle').length > 0`,
    );
    if (ready) return true;
    await delay(300);
  }
  return false;
}

/** 读取详情页上的流程状态，用来在日志里交叉验证推进是否符合预期 */
async function readCurrentStep() {
  return evaluate(`
    (() => {
      const text = document.querySelector('.ant-descriptions')?.innerText ?? '';
      return text.replace(/\\s+/g, ' ').trim().slice(0, 160);
    })()
  `);
}

/**
 * 读取铃铛上的未读数。
 *
 * 不能用 textContent：antd 的数字滚动动画会把相邻数字一起渲染进 DOM，
 * 读出来会变成「34」这种拼串。title 属性才是准确值。
 */
async function readBadge() {
  return evaluate(`
    (() => {
      const badge = document.querySelector('.notify-trigger .ant-badge-count');
      if (!badge) return 0;
      const title = badge.getAttribute('title');
      if (title) return Number(title.trim()) || 0;
      return Number(badge.textContent.trim()) || 0;
    })()
  `);
}

/**
 * 用 Node 侧的独立会话调接口，不动浏览器里的登录态。
 * 这样可以在「浏览器正以调度员身份开着」的同时，让另一个人制造事件。
 */
async function apiLogin(username, password) {
  const response = await fetch(`${BASE}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!response.ok) throw new Error(`接口登录失败：${username}`);
  return response.headers.getSetCookie().map((item) => item.split(';')[0]).join('; ');
}

async function apiFetch(path, cookie, options = {}) {
  const response = await fetch(`${BASE}${path}`, {
    ...options,
    headers: {
      'content-type': 'application/json',
      cookie,
      ...(options.headers ?? {}),
    },
  });
  const text = await response.text();
  return text ? JSON.parse(text) : null;
}

/** 切换登录身份，用于依次扮演流程里的不同角色 */
async function becomeUser(username, password) {
  return evaluate(`
    (async () => {
      const post = (url, body) =>
        fetch(url, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          credentials: 'same-origin',
          body: JSON.stringify(body),
        });

      let response = await post('/api/auth/login', {
        username: ${JSON.stringify(username)},
        password: ${JSON.stringify(password)},
      });
      if (!response.ok) return 'login-failed';

      // 新账号首次登录强制改密，脚本里一并处理掉
      const me = await (await fetch('/api/auth/me', { credentials: 'same-origin' })).json();
      if (me.mustChangePassword) {
        await post('/api/auth/change-password', {
          oldPassword: ${JSON.stringify(password)},
          newPassword: 'pass123456',
        });
      }
      return 'ok';
    })()
  `);
}

/** 往 textarea 里填值，同样要走原生 setter 才能让 React 收到 */
async function fillTextarea(index, value) {
  return evaluate(`
    (() => {
      const areas = document.querySelectorAll('textarea');
      const area = areas[${index}];
      if (!area) return false;
      const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
      setter.call(area, ${JSON.stringify(value)});
      area.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    })()
  `);
}

console.log(`目标地址 ${BASE}`);
await goto('/login');
await shot('01-login');

if (!(await evaluate(fillInputs([USER, PASS])))) {
  throw new Error('登录页没有渲染出输入框');
}
await submitForm();
await delay(2500);
await shot('02-after-login');

// 全新账号或被重置过密码时，会先落到强制改密页，这里把它走完才能进主界面
if (await evaluate(`document.body.innerText.includes('设置新密码')`)) {
  console.log('  命中强制改密流程');
  if (!(await evaluate(fillInputs([PASS, NEW_PASS, NEW_PASS])))) {
    throw new Error('改密页没有渲染出输入框');
  }
  await submitForm();
  await delay(2500);
  await shot('03-after-change-password');
}

console.log(`  当前页面 ${await evaluate('window.location.pathname')}`);

// 导入流程需要先有字段定义。字段页面本身下面会单独截图，这里直接调接口把前置数据备好。
await evaluate(`
  (async () => {
    const list = await (await fetch('/api/fields', { credentials: 'same-origin' })).json();
    const has = (code) => Array.isArray(list) && list.some((f) => f.code === code);
    const create = (body) => fetch('/api/fields', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify(body),
    });
    if (!has('code')) await create({ code: 'code', label: '编号', fieldType: 'text', required: true, isUniqueKey: true, sort: 1 });
    if (!has('name')) await create({ code: 'name', label: '名称', fieldType: 'text', sort: 2 });
    if (!has('qty')) await create({ code: 'qty', label: '数量', fieldType: 'number', sort: 3 });
    return true;
  })()
`);

for (const page of PAGES) {
  await goto(page.path);
  await shot(page.name);

  // 二级界面（抽屉）单独走一遍，确认表单绑定真的生效
  if (page.path === '/admin/roles') {
    if (await clickByText('配置权限')) {
      await delay(1200);
      await shot('07b-role-permissions');
    } else {
      console.log('  ⚠ 没找到「配置权限」入口');
    }
  }
}

// ---- 导入流程走查 ----
console.log('\n导入流程：');
await goto('/data/import');
await shot('08-import-upload');

// 第 3 行故意留空编号，用来验证「错误行」的呈现
const pasted = [
  '编号\t名称\t数量',
  'A-1\t螺丝\t10',
  'A-2\t螺母\t20',
  '\t垫片\t30',
].join('\n');

if (!(await fillTextarea(0, pasted))) {
  throw new Error('导入页没有渲染出粘贴框');
}
await clickByText('解析粘贴的内容');
await delay(1800);
await shot('09-import-mapping');

if (!(await clickByText('下一步：预览校验结果'))) {
  throw new Error('没有找到预览按钮');
}
await delay(1800);
await shot('10-import-preview');

if (!(await clickByText('确认导入'))) {
  throw new Error('没有找到确认导入按钮');
}
await delay(2000);
await shot('11-import-result');

// 导入完成后看看数据列表
await goto('/records');
await shot('12-records');

// ---------------- 流转全流程 ----------------
console.log('\n流转流程：');

// 先备齐调度员和承办人，并让脚本能切换身份
await becomeUser(USER, NEW_PASS);
await evaluate(`
  (async () => {
    const get = (url) => fetch(url, { credentials: 'same-origin' }).then((r) => r.json());
    const roles = await get('/api/roles');
    const roleId = (code) => roles.find((role) => role.code === code)?.id;
    const users = await get('/api/users');
    const exists = (username) => users.some((user) => user.username === username);
    const create = async (username, displayName, roleCode) => {
      if (exists(username)) return;
      await fetch('/api/users', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({
          username,
          displayName,
          password: 'init123456',
          roleIds: [roleId(roleCode)],
        }),
      });
    };
    await create('dispatcher1', '调度员甲', 'dispatcher');
    await create('handler1', '承办甲', 'handler');
    await create('handler2', '承办乙', 'handler');
    return true;
  })()
`);

// 1) 管理员发起
await goto('/flows');
await shot('13-flow-center');

if (!(await clickByText('发起流转'))) {
  throw new Error('没有找到发起流转按钮');
}
await delay(1200);

// 弹窗里的标题输入框
await evaluate(`
  (() => {
    const input = [...document.querySelectorAll('.ant-modal input')].find(
      (el) => (el.placeholder ?? '').includes('一句话'),
    );
    if (!input) return false;
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(input, '三月份设备报修');
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()
`);
await delay(300);
await shot('14-flow-start-modal');
await clickModalOk();
await delay(2000);
await shot('15-flow-detail-as-initiator');

// 2) 调度员分派
await becomeUser('dispatcher1', 'init123456');
await goto('/flows');
await shot('16-flow-todo-dispatcher');

if (!(await clickByText('去处理'))) {
  throw new Error('调度员没有看到待办');
}
await delay(1800);
await shot('17-flow-detail-as-dispatcher');

if (!(await clickByText('分派给承办人'))) {
  throw new Error('没有找到分派按钮');
}
await delay(1000);

// 依次选中两位承办人
console.log(`  指派：${await pickAssignees(['承办甲', '承办乙'])}`);
await fillTextarea(0, '请两位一起看一下，尽快反馈');
await delay(300);
await shot('18-flow-dispatch-modal');
await clickModalOk();
await delay(2000);

// 3) 两位承办人都要处理，全部完成规则下缺一个流程不会推进
await becomeUser('handler1', 'init123456');
await goto('/flows');
await mustClick('去处理', '承办甲没有看到待办');
await delay(1500);
await shot('19-flow-detail-as-handler');

await mustClick('提交承办结果', '没有找到提交按钮');
await delay(1000);
await fillTextarea(0, '已现场处理完毕');
await clickModalOk();
await delay(2000);

console.log(`  承办甲完成后的环节：${await readCurrentStep()}`);

// 承办乙随后完成，流程才应该进入确认
await becomeUser('handler2', 'init123456');
await goto('/flows');
await mustClick('去处理', '承办乙没有看到待办');
await delay(1500);
await mustClick('提交承办结果', '没有找到提交按钮');
await delay(1000);
await fillTextarea(0, '已配合处理');
await clickModalOk();
await delay(2000);

// 4) 确认打回，验证回退确实退回分派步骤
await becomeUser('dispatcher1', 'pass123456');
await goto('/flows');
await mustClick('去处理', '调度员没有收到确认任务');
await delay(1500);
await mustClick('打回重做', '没有找到打回按钮');
await delay(1000);
await fillTextarea(0, '照片没上传，退回重做');
await shot('20-flow-reject-modal');
await clickModalOk();
await delay(2200);
await shot('21-flow-after-reject');

console.log(`  打回后当前环节：${await readCurrentStep()}`);

// 5) 重新分派 → 承办 → 确认通过
await mustClick('分派给承办人', '打回后没有重新出现分派按钮');
await delay(1000);
console.log(`  指派：${await pickAssignees(['承办甲'])}`);
await fillTextarea(0, '这次只看甲的处理');
await clickModalOk();
await delay(2000);

await becomeUser('handler1', 'pass123456');
await goto('/flows');
await mustClick('去处理', '承办甲没有收到重做的任务');
await delay(1500);
await mustClick('提交承办结果', '没有找到提交按钮');
await delay(1000);
await fillTextarea(0, '已补齐照片');
await clickModalOk();
await delay(2000);

await becomeUser('dispatcher1', 'pass123456');
await goto('/flows');
await mustClick('去处理', '调度员没有收到确认任务');
await delay(1500);
await mustClick('确认通过', '没有找到确认通过按钮');
await delay(1000);
await fillTextarea(0, '没问题，结案');
await clickModalOk();
await delay(2500);
await shot('22-flow-completed');

console.log(`  最终状态：${await readCurrentStep()}`);

// ---------------- 实时消息 ----------------
console.log('\n实时消息：');

await becomeUser('dispatcher1', 'pass123456');
await goto('/flows');

const beforeBadge = await readBadge();
console.log(`  推送前未读：${beforeBadge}`);

// Node 侧以管理员身份另开一个会话发起事项，浏览器这边保持调度员不动
const adminCookie = await apiLogin(USER, NEW_PASS);
const defs = await apiFetch('/api/flows/defs', adminCookie);
await apiFetch('/api/flows', adminCookie, {
  method: 'POST',
  body: JSON.stringify({ defId: defs[0].id, title: '推送验证事项' }),
});

let afterBadge = beforeBadge;
const badgeDeadline = Date.now() + 8000;
while (Date.now() < badgeDeadline && afterBadge <= beforeBadge) {
  await delay(300);
  afterBadge = await readBadge();
}
console.log(`  推送后未读：${afterBadge}`);
if (afterBadge <= beforeBadge) {
  throw new Error(`实时推送没有生效：未读数没有变化（${beforeBadge} → ${afterBadge}）`);
}

// 触发器是 Popover 的子元素，按下去要带完整事件序列才会展开
await evaluate(`
  (() => {
    const trigger = document.querySelector('.notify-trigger');
    if (!trigger) return false;
    for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
      trigger.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }));
    }
    return true;
  })()
`);
await delay(900);
await shot('23-notification-panel');

const panelText = await evaluate(
  `document.querySelector('.notify-panel')?.innerText.replace(/\\s+/g, ' ').slice(0, 100) ?? ''`,
);
console.log(`  消息面板：${panelText}`);

await goto('/notifications');
await shot('24-notifications-page');

// ---------------- 统计 ----------------
console.log('\n统计：');

await becomeUser(USER, NEW_PASS);
await goto('/stats');
console.log(`  图表就绪：${await waitForChart()}`);
await shot('25-stats-global');

// 换个短窗口看看柱子是否只是太细
await evaluate(`
  (() => {
    const target = [...document.querySelectorAll('.ant-segmented-item-label')].find(
      (el) => el.textContent.trim() === '近 7 天',
    );
    if (!target) return false;
    target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    return true;
  })()
`);
await waitForChart();
await shot('25b-stats-7days');

// 顺带把图表的实际渲染结果读出来，确认柱子有没有画
console.log(`  图表柱子：${await evaluate(`
  (() => {
    const bars = [...document.querySelectorAll('.recharts-bar-rectangle path, .recharts-bar-rectangle rect')];
    if (bars.length === 0) return '没有柱子元素';
    const heights = bars.map((el) => Number(el.getAttribute('height') ?? 0)).filter((h) => h > 0);
    return bars.length + ' 个元素，其中高度大于 0 的 ' + heights.length + ' 个';
  })()
`)}`);

console.log(`  全局概览：${await evaluate(
  `[...document.querySelectorAll('.ant-statistic')].map((el) => el.innerText.replace(/\\s+/g, ' ')).slice(0, 4).join(' | ')`,
)}`);

// 切到「我的」看个人视角
await evaluate(`
  (() => {
    const target = [...document.querySelectorAll('.ant-segmented-item-label')].find(
      (el) => el.textContent.trim() === '我的',
    );
    if (!target) return false;
    target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    return true;
  })()
`);
console.log(`  图表就绪：${await waitForChart()}`);
await shot('26-stats-mine');

if (consoleErrors.length > 0) {
  console.log('\n浏览器控制台报错：');
  for (const error of consoleErrors.slice(0, 10)) {
    console.log(`  - ${error}`);
  }
} else {
  console.log('\n浏览器控制台无报错');
}

socket.close();
cleanup();
