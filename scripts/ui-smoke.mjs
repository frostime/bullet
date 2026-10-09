import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const server = await createServer({ server: { host: '127.0.0.1', port: 1420, strictPort: false } });
await server.listen();
const address = server.httpServer.address();
const url = `http://127.0.0.1:${address.port}`;
const browser = await chromium.launch({ executablePath: process.env.BULLET_TEST_CHROMIUM || undefined,
  args: ['--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu'], headless: true });
try {
  const page = await browser.newPage({viewport:{width:1280,height:900}});
  const errors=[]; page.on('pageerror',error=>errors.push(error.message));
  await page.goto(url);
  await page.getByRole('button',{name:'展开或收起 bullet'}).click();
  const input = page.getByRole('textbox',{name:'输入文本'});
  await input.fill('第一行'); await input.press('Enter');
  assert.equal(await input.inputValue(),'第一行\n');
  assert.equal(await page.getByText('后台运行 ↗',{exact:true}).count(),0);
  await input.fill('并行任务演示 '.repeat(15)); await input.press('Control+Enter');
  await page.getByText('后台运行 ↗',{exact:true}).click();
  assert.equal(await input.inputValue(),'');
  await input.fill('  你好，bullet。  '); await input.press('Control+Enter');
  await page.getByText('新输入',{exact:true}).waitFor();
  assert.equal((await page.locator('.result-scroll pre').textContent()).trim(),'你好，bullet。');
  await fs.mkdir('screenshots',{recursive:true});
  await page.screenshot({path:'screenshots/input-result.png'});
  await page.getByRole('button',{name:'配置动作',exact:true}).click();
  await page.getByRole('heading',{name:'配置动作',exact:true}).waitFor();
  await page.getByRole('textbox',{name:'脚本源码'}).fill('import sys\nprint(sys.stdin.read().upper())\n');
  await page.getByRole('button',{name:'保存脚本',exact:true}).click();
  await page.getByText('脚本已保存',{exact:true}).waitFor();
  const sidebarAction=page.locator('.actions-sidebar>button').first(); await sidebarAction.click();
  assert.match(await page.getByRole('textbox',{name:'脚本源码'}).inputValue(),/upper/);
  await page.screenshot({path:'screenshots/settings.png'});
  await page.getByRole('button',{name:'任务',exact:true}).click();
  assert.equal(await page.locator('.task-row').count(),2);
  await page.screenshot({path:'screenshots/tasks.png'});
  await page.getByRole('button',{name:'返回桌面 ↗',exact:true}).click();
  await input.press('Escape');
  assert.equal(await input.isVisible(),false);
  await page.getByRole('button',{name:'展开或收起 bullet'}).click();
  assert.equal(await input.inputValue(),'');

  // Many actions must remain reachable, independent of result scrolling.
  const defaults=JSON.parse(await fs.readFile('src-tauri/defaults.json','utf8'));
  defaults.actions = Array.from({length:30},(_,index)=>({...defaults.actions[0],id:`demo-${index}`,name:`动作 ${index+1}`}));
  await page.evaluate(value=>localStorage.setItem('bullet-demo-config',JSON.stringify(value)),defaults);
  await page.reload(); await page.getByRole('button',{name:'展开或收起 bullet'}).click();
  await page.locator('.action-select').click();
  const menu=page.locator('.action-menu-scroll');
  assert.equal(await menu.evaluate(element=>element.scrollHeight>element.clientHeight),true);
  await menu.evaluate(element=>element.scrollTop=element.scrollHeight);
  await page.getByRole('button',{name:'动作 30 SCRIPT',exact:true}).click();
  assert.match(await page.locator('.action-select').textContent(),/动作 30/);
  await input.fill('长输出\n'.repeat(150)); await input.press('Control+Enter');
  await page.waitForTimeout(1600);
  await page.locator('.result-scroll').waitFor();
  assert.equal(await page.locator('.result-scroll').evaluate(element=>element.scrollHeight>element.clientHeight),true);
  await page.setViewportSize({width:680,height:600});
  await page.getByRole('button',{name:'配置动作',exact:true}).click();
  const content=page.locator('.management-content');
  assert.equal(await content.evaluate(element=>element.scrollHeight>element.clientHeight),true);
  assert.deepEqual(errors,[]);
  console.log('UI smoke passed: Enter/Ctrl+Enter, detach/concurrency, result, script editor, task routing, collapse reset, 30-action menu, output and small-window scrolling.');
} finally { await browser.close(); await server.close(); }
