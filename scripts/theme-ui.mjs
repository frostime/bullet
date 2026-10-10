import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const server = await createServer({ server: { host: '127.0.0.1', port: 1435, strictPort: false } });
await server.listen();
const url = `http://127.0.0.1:${server.httpServer.address().port}`;
const browser = await chromium.launch({ headless: true, executablePath: process.env.BULLET_TEST_CHROMIUM || undefined });
const errors = [];
const watch = page => page.on('pageerror', error => errors.push(error.message));
const expectTheme = (page, theme) => page.waitForFunction(value => document.documentElement.dataset.theme === value, theme);
try {
  const context = await browser.newContext({ viewport: { width: 1100, height: 850 } });
  const page = await context.newPage();
  watch(page);
  await page.goto(url + '/?surface=manage#settings');
  const name = page.getByLabel('名称', { exact: true });
  await name.fill('未保存的主题切换草稿');
  await fs.mkdir('tmp/theme-ui', { recursive: true });
  await fs.writeFile('tmp/.gitignore', '*\n');
  await page.screenshot({ path: 'tmp/theme-ui/light.png' });
  await page.getByRole('button', { name: '切换到暗色主题', exact: true }).click();
  await expectTheme(page, 'dark');
  assert.equal(await name.inputValue(), '未保存的主题切换草稿');
  const colors = await page.locator('.management-shell').evaluate(element => {
    const style = getComputedStyle(element);
    return { background: style.backgroundColor, text: style.color };
  });
  assert.deepEqual(colors, { background: 'rgb(25, 25, 25)', text: 'rgb(237, 237, 237)' });
  assert.equal(await page.getByLabel('脚本源码').evaluate(element => getComputedStyle(element).backgroundColor), 'rgb(34, 34, 34)');
  await page.screenshot({ path: 'tmp/theme-ui/dark.png' });
  await page.getByRole('button', { name: '任务', exact: true }).click();
  await expectTheme(page, 'dark');
  await page.getByRole('button', { name: '动作', exact: true }).click();
  assert.equal(await name.inputValue(), '未保存的主题切换草稿');

  // Independent surfaces restore the same choice and follow cross-tab changes.
  const panel = await context.newPage();
  const bubble = await context.newPage();
  watch(panel); watch(bubble);
  await panel.goto(url + '/?surface=panel');
  await bubble.goto(url + '/?surface=bubble');
  await expectTheme(panel, 'dark');
  await expectTheme(bubble, 'dark');
  assert.equal(await panel.locator('.send-button').evaluate(element => getComputedStyle(element).color), 'rgb(25, 25, 25)');
  await panel.screenshot({ path: 'tmp/theme-ui/panel-dark.png' });
  await page.reload();
  await expectTheme(page, 'dark');
  await page.getByRole('button', { name: '切换到亮色主题', exact: true }).click();
  await expectTheme(panel, 'light');
  await expectTheme(bubble, 'light');

  // Native IPC simulation: committed state, failures, events, and a late initial read.
  // This is not an installed Windows/WebView acceptance test.
  const nativePage = await browser.newPage();
  watch(nativePage);
  await nativePage.addInitScript(() => {
    const callbacks = {};
    const listeners = {};
    let callbackId = 0;
    window.isTauri = true;
    const state = window.testTheme = {
      current: 'dark', failSave: false, commits: 0, finishRead: null,
      emit(theme) { state.current = theme; callbacks[listeners['theme-changed']]?.({ payload: theme }); }
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'manage' }, currentWebview: { label: 'manage' } },
      transformCallback(fn) { callbacks[++callbackId] = fn; return callbackId; },
      unregisterCallback() {},
      async invoke(command, args) {
        switch (command) {
          case 'plugin:event|listen': listeners[args.event] = args.handler; return callbackId;
          case 'plugin:event|unlisten': return;
          case 'get_theme': return new Promise(resolve => { state.finishRead = resolve; });
          case 'set_theme':
            if (state.failSave) throw 'state.json：replace：拒绝访问';
            state.commits++;
            state.emit(args.theme);
            return state.current;
          case 'list_runs': return [];
          default: throw 'unexpected command: ' + command;
        }
      }
    };
  });
  await nativePage.goto(url + '/?surface=manage#tasks');
  await nativePage.waitForFunction(() => window.testTheme.finishRead !== null);
  await nativePage.evaluate(() => { testTheme.emit('dark'); testTheme.finishRead('light'); });
  await expectTheme(nativePage, 'dark');
  await nativePage.getByRole('button', { name: '切换到亮色主题', exact: true }).waitFor();
  await nativePage.evaluate(() => { testTheme.failSave = true; });
  await nativePage.getByRole('button', { name: '切换到亮色主题', exact: true }).click();
  await nativePage.getByRole('alert').waitFor();
  await expectTheme(nativePage, 'dark');
  assert.match(await nativePage.getByRole('alert').textContent(), /拒绝访问/);
  assert.equal(await nativePage.evaluate(() => testTheme.commits), 0);
  await nativePage.evaluate(() => { testTheme.failSave = false; });
  await nativePage.getByRole('button', { name: '切换到亮色主题', exact: true }).click();
  await expectTheme(nativePage, 'light');
  assert.equal(await nativePage.getByRole('alert').count(), 0);
  assert.equal(await nativePage.evaluate(() => testTheme.commits), 1);
  assert.deepEqual(errors, []);
  console.log('Theme UI passed: both palettes, unsaved drafts, routes, reload, shared surfaces, IPC commit/failure, and late-load event ordering.');
} finally {
  await browser.close();
  await server.close();
}
