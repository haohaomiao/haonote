// Run inside xvfb-run + dbus-run-session. Uses an isolated data directory, never your real notes.
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import assert from 'node:assert/strict';

const data = await mkdtemp(join(tmpdir(), 'qingnote-native-'));
const env = {
  ...process.env,
  XDG_DATA_HOME: join(data, 'data'),
  XDG_CONFIG_HOME: join(data, 'config'),
  XDG_CACHE_HOME: join(data, 'cache'),
  WEBKIT_DISABLE_DMABUF_RENDERER: '1',
  XDG_CURRENT_DESKTOP: 'OPENBOX',
};
let wmLog = '';
const wm = spawn(process.env.OPENBOX_PATH || 'openbox', ['--sm-disable'], {
  env,
  stdio: ['ignore', 'pipe', 'pipe'],
});
const compositor = process.env.XCOMPMGR_PATH
  ? spawn(process.env.XCOMPMGR_PATH, ['-n'], { env, stdio: 'ignore' })
  : null;
wm.stdout.on('data', (chunk) => (wmLog += chunk));
wm.stderr.on('data', (chunk) => (wmLog += chunk));
wm.on('error', (error) => {
  console.error('Native tests require Openbox:', error.message);
});
const driver = spawn(
  process.env.TAURI_DRIVER || 'tauri-driver',
  [
    '--port',
    '4444',
    '--native-port',
    '4445',
    ...(process.env.WEBKIT_DRIVER ? ['--native-driver', process.env.WEBKIT_DRIVER] : []),
  ],
  { env, stdio: ['ignore', 'pipe', 'pipe'] },
);
let driverLog = '';
driver.stdout.on('data', (chunk) => (driverLog += chunk));
driver.stderr.on('data', (chunk) => (driverLog += chunk));
let session;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function request(path, method = 'GET', body) {
  const response = await fetch(`http://127.0.0.1:4444${path}`, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(45000),
  });
  const json = await response.json();
  if (!response.ok || json.value?.error) throw new Error(JSON.stringify(json));
  return json.value;
}
const wd = (path, method, body) => request(`/session/${session}${path}`, method, body);
async function until(fn, description) {
  for (let i = 0; i < 80; i++) {
    const result = await fn();
    if (result) return result;
    await sleep(100);
  }
  throw new Error(`Timed out: ${description}`);
}
const execute = (script, args = []) => wd('/execute/sync', 'POST', { script, args });
const toggleBody = () =>
  execute(
    `document.querySelector('.drag-handle').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));`,
  );
const invoke = (command, args = {}) =>
  wd('/execute/async', 'POST', {
    script: `const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(v=>done({value:v})).catch(e=>done({error:String(e)}));`,
    args: [command, args],
  }).then((result) => {
    if (result.error) throw new Error(result.error);
    return result.value;
  });
async function click(label) {
  const optionsOpen =
    label === '便签选项'
      ? await execute('return !!document.querySelector(".editor-options")')
      : undefined;
  await until(
    () =>
      execute(
        `return [...document.querySelectorAll('button')].some(b=>!b.disabled && (b.getAttribute('aria-label')===arguments[0]||b.textContent.includes(arguments[0])));`,
        [label],
      ),
    `button ready: ${label}`,
  );
  await execute(
    `const b=[...document.querySelectorAll('button')].find(b=>b.getAttribute('aria-label')===arguments[0]||b.textContent.includes(arguments[0]));if(!b)throw new Error('No button: '+arguments[0]);b.click();`,
    [label],
  );
  if (optionsOpen !== undefined) {
    await until(
      async () =>
        (await execute('return !!document.querySelector(".editor-options")')) !== optionsOpen,
      'options menu toggle settled',
    );
  }
}
async function createSession() {
  const result = await request('/session', 'POST', {
    capabilities: {
      alwaysMatch: {
        'tauri:options': {
          application: resolve(process.env.QINGNOTE_BINARY || 'target/debug/qingnote'),
        },
      },
    },
  });
  session = result.sessionId;
  await until(
    () => execute('return !!document.querySelector(".library-shell")'),
    'main window ready',
  );
  return wd('/window');
}
try {
  await until(async () => {
    try {
      await request('/status');
      return true;
    } catch {
      return false;
    }
  }, 'driver');
  let main = await createSession();
  assert.deepEqual(await invoke('list_notes'), []);
  await click('写下第一张便签');
  const handles = await until(async () => {
    const handles = await wd('/window/handles');
    return handles.length === 2 && handles;
  }, 'independent note window');
  await wd('/window', 'POST', { handle: handles.find((h) => h !== main) });
  await click('切换到 Markdown 源码');
  await until(
    () => execute('return !!document.querySelector("textarea:not(:disabled)")'),
    'editor ready',
  );
  await execute(
    `const area=document.querySelector('textarea');area.value=arguments[0];area.dispatchEvent(new Event('input',{bubbles:true}));`,
    ['原生窗口测试\n中文输入与自动保存'],
  );
  await click('收起便签');
  await wd('/window', 'POST', { handle: main });
  await until(
    async () => (await wd('/window/handles')).length === 1,
    'editor closes after flushing',
  );
  let notes = await invoke('list_notes');
  assert.equal(notes.length, 1);
  assert.equal(notes[0].content.text, '原生窗口测试\n中文输入与自动保存');
  await click('打开便签：原生窗口测试');
  const second = await until(async () => {
    const h = await wd('/window/handles');
    return h.length === 2 && h.find((h) => h !== main);
  }, 'reopen note');
  await wd('/window', 'POST', { handle: second });
  await until(
    () => execute('return document.querySelector("textarea")?.value.includes("中文输入")'),
    'restored text',
  );
  await click('窗口置顶');
  await sleep(300);
  const windowList = execFileSync('xprop', ['-root', '_NET_CLIENT_LIST'], {
    env,
    encoding: 'utf8',
  });
  const properties = (windowList.match(/0x[0-9a-f]+/g) || []).map((xid) =>
    execFileSync('xprop', ['-id', xid, '_NET_WM_NAME', '_NET_WM_STATE'], { env, encoding: 'utf8' }),
  );
  assert.ok(
    properties.some((p) => p.includes('"轻笺"') && p.includes('_NET_WM_STATE_ABOVE')),
    'the actual X11 note window must be above other windows',
  );
  await until(() => invoke('get_pinned'), 'pin window');
  // Real native dimensions, not just a hidden DOM body.
  await click('便签选项');
  await execute(`
    for (const [label,value] of [['便签宽度','420'],['便签高度','460']]) {
      const input=document.querySelector('[aria-label="'+label+'"]');
      input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));
    }
  `);
  await click('应用大小');
  await until(
    () => execute('return innerWidth===420 && innerHeight===460'),
    'explicit native size',
  );
  await execute(
    `const slider=document.querySelector('[aria-label="便签不透明度"]');slider.value='65';slider.dispatchEvent(new Event('input',{bubbles:true}));slider.dispatchEvent(new Event('change',{bubbles:true}));`,
  );
  await until(async () => (await invoke('get_note_view')).opacity === 65, 'opacity persisted');
  await click('便签选项');
  await execute(
    `document.querySelector('.drag-handle').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));`,
  );
  await until(
    () => execute('return innerHeight===40 && !document.querySelector("textarea")'),
    'collapsed native height',
  );
  // A native popup remains usable even when the WebView is only a title strip.
  await execute(
    `document.querySelector('.editor-bar').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:100,clientY:20}));`,
  );
  await sleep(600);
  execFileSync(
    process.env.XDOTOOL_PATH || 'xdotool',
    ['key', '--clearmodifiers', 'Down', 'Return'],
    { env },
  );
  await until(
    () =>
      execute(
        'return innerHeight===460 && innerWidth===420 && !!document.querySelector("textarea")',
      ),
    'expanded size restored',
  );
  await click('切换到排版编辑');
  await until(
    () => execute('return !!document.querySelector(".markdown-preview")'),
    'native Markdown preview',
  );
  await toggleBody();
  await until(() => execute('return innerHeight===40'), 'collapse before closing');
  await click('便签选项');
  await click('鼠尾草绿');
  await toggleBody();
  await until(() => execute('return innerHeight===40'), 'collapsed state before restart');
  await click('收起便签');
  await wd('/window', 'POST', { handle: main });
  await until(
    async () => (await invoke('list_notes'))[0].content.color === 'sage',
    'color persisted',
  );
  await click('设置与同步');
  await until(() => execute('return !!document.querySelector("dialog[open]")'), 'native settings');
  const settings = await invoke('get_settings');
  assert.equal(settings.config, null);
  assert.ok(settings.dataDirectory.startsWith(data));
  await mkdir('artifacts', { recursive: true });
  await writeFile('artifacts/native-settings.png', Buffer.from(await wd('/screenshot'), 'base64'));
  await click('关闭设置');
  await writeFile('artifacts/native-library.png', Buffer.from(await wd('/screenshot'), 'base64'));
  // Explicitly exit, then launch a fresh process against the same isolated SQLite database.
  await execute(`setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit_app'),100);`);
  await sleep(400);
  try {
    await wd('', 'DELETE');
  } catch {
    /* window already closed */
  }
  main = await createSession();
  notes = await invoke('list_notes');
  assert.equal(notes[0].content.text, '原生窗口测试\n中文输入与自动保存');
  assert.equal(notes[0].content.color, 'sage');
  await click('打开便签：原生窗口测试');
  const editorHandle = await until(async () => {
    const handles = await wd('/window/handles');
    return handles.length === 2 && handles.find((handle) => handle !== main);
  }, 'editor after restart');
  await wd('/window', 'POST', { handle: editorHandle });
  await until(
    () => execute('return document.querySelector(".editor")?.classList.contains("collapsed")'),
    'collapsed state restored after restart',
  );
  assert.equal(await execute('return innerHeight'), 40);
  assert.deepEqual(await invoke('get_note_view'), {
    collapsed: true,
    opacity: 65,
    markdown: true,
    fontFamily: 'sans',
    fontSize: 17,
  });
  await toggleBody();
  await until(
    () =>
      execute(
        'return innerHeight===460 && innerWidth===420 && !!document.querySelector(".markdown-preview")',
      ),
    'expanded dimensions and Markdown restored',
  );
  assert.equal(
    await execute('return getComputedStyle(document.querySelector(".editor")).opacity'),
    '0.65',
  );
  await click('切换到 Markdown 源码');
  await until(
    () => execute('return !!document.querySelector("textarea:not(:disabled)")'),
    'editor after restart ready',
  );
  assert.equal(await invoke('get_pinned'), true);
  await click('便签选项');
  await execute(
    `const input=document.querySelector('[aria-label="便签标题"]');input.value='独立原生标题';input.dispatchEvent(new Event('input',{bubbles:true}));`,
  );
  await execute(
    `document.querySelector('textarea').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}));`,
  );
  await until(
    () => execute('return !document.querySelector(".editor-options")'),
    'click outside dismisses options',
  );
  await click('便签选项');
  await execute(
    `const input=document.querySelector('[aria-label="正文字体"]');input.value='mono';input.dispatchEvent(new Event('change',{bubbles:true}));`,
  );
  await until(async () => (await invoke('get_note_view')).fontFamily === 'mono', 'font saved');
  await click('便签选项');
  await execute(
    `const area=document.querySelector('textarea');area.value='工具测试';area.dispatchEvent(new Event('input',{bubbles:true}));area.focus();area.setSelectionRange(0,4);`,
  );
  await click('便签选项');
  await click('加粗');
  await click('斜体');
  await click('下划线');
  await click('便签选项');
  await until(
    () => execute('return document.querySelector("textarea").value === "**_<u>工具测试</u>_**"'),
    'native selection formatting',
  );
  await click('切换到排版编辑');
  await until(
    () =>
      execute(
        'return document.querySelector(".markdown-preview strong em u")?.textContent === "工具测试"',
      ),
    'native formatted preview',
  );
  await writeFile(
    'artifacts/native-text-tools.png',
    Buffer.from(await wd('/screenshot'), 'base64'),
  );
  // Format the actual rich selection through the OS menu, then type directly into it.
  await execute(
    `const el=document.querySelector('.tiptap');el.focus();const range=document.createRange();range.selectNodeContents(el);const selection=getSelection();selection.removeAllRanges();selection.addRange(range);`,
  );
  await sleep(100);
  await execute(
    `document.querySelector('.tiptap').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,clientX:80,clientY:80}));`,
  );
  await sleep(600);
  execFileSync(
    process.env.XDOTOOL_PATH || 'xdotool',
    ['key', '--clearmodifiers', 'Down', 'Return'],
    { env },
  );
  await until(
    () => execute('return !document.querySelector(".tiptap strong")'),
    'native rich menu toggles bold on the original selection',
  );
  await execute(
    `const el=document.querySelector('.tiptap');el.focus();document.execCommand('insertText',false,'原生排版编辑');`,
  );
  await until(
    () => execute('return document.querySelector(".tiptap").textContent.includes("原生排版编辑")'),
    'native direct rich editing',
  );
  await click('切换到 Markdown 源码');
  await until(
    () => execute('return !!document.querySelector("textarea")'),
    'source editor ready after rich editing',
  );
  await execute(
    `const area=document.querySelector('textarea');area.value=arguments[0];area.dispatchEvent(new Event('input',{bubbles:true}));`,
    ['退出前的最后一笔'],
  );
  await wd('/window', 'POST', { handle: main });
  await click('退出轻笺');
  await until(async () => {
    try {
      return (await wd('/window/handles')).length === 0;
    } catch {
      return true;
    }
  }, 'normal quit after flushing editors');
  try {
    await wd('', 'DELETE');
  } catch {
    /* application exited normally */
  }
  main = await createSession();
  notes = await invoke('list_notes');
  assert.equal(notes[0].content.text, '退出前的最后一笔');
  assert.equal(notes[0].content.title, '独立原生标题');
  const restoredEditor = (await wd('/window/handles')).find((handle) => handle !== main);
  assert.ok(restoredEditor);
  await wd('/window', 'POST', { handle: restoredEditor });
  await until(
    async () => (await invoke('get_note_view')).fontFamily === 'mono',
    'font restored after restart',
  );
  await execute(`setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit_app'),100);`);
  await sleep(300);
  console.log(
    'PASS: native persistence, flush-on-close/quit, pinning, colors, collapse, resize, opacity, Markdown, and restart',
  );
  console.log(`Isolated test data: ${data}`);
} catch (error) {
  if (session) {
    try {
      await mkdir('artifacts', { recursive: true });
      await writeFile(
        'artifacts/native-failure.png',
        Buffer.from(await wd('/screenshot'), 'base64'),
      );
      console.error(
        await execute('return {text:document.body.innerText,width:innerWidth,height:innerHeight}'),
      );
    } catch {
      /* window gone */
    }
  }
  console.error('Window manager:', wmLog);
  console.error(driverLog.slice(-6000));
  throw error;
} finally {
  if (session) {
    try {
      await execute(`setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit_app'),50);`);
      await sleep(100);
    } catch {
      /* no remaining test window */
    }
  }
  if (session) {
    try {
      await wd('', 'DELETE');
    } catch {
      /* process may already have exited */
    }
  }
  driver.kill('SIGTERM');
  compositor?.kill('SIGTERM');
  wm.kill('SIGTERM');
}
