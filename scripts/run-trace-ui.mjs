import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createServer} from 'vite';
import {chromium} from 'playwright';

const server = await createServer({server:{host:'127.0.0.1',port:1434,strictPort:false}});
await server.listen();
const url = `http://127.0.0.1:${server.httpServer.address().port}`;
const browser = await chromium.launch({headless:true,executablePath:process.env.BULLET_TEST_CHROMIUM || undefined});
try {
  const page = await browser.newPage();
  const errors = []; page.on('pageerror',error=>errors.push(error.message));
  const demoConfig = JSON.parse(await fs.readFile('src-tauri/defaults.json','utf8'));
  demoConfig.actions[0].env.API_KEY = 'synthetic-secret-not-real';
  await page.addInitScript(config=>localStorage.setItem('bullet-demo-config',JSON.stringify(config)),demoConfig);
  await page.goto(url);
  const bubble = page.getByRole('button',{name:'展开或收起 bullet'});
  await bubble.click();
  const input = page.getByRole('textbox',{name:'输入文本'});
  assert.equal(await page.locator('.composer-footer .quiet').textContent(),'任务');
  await input.fill('后台结果 '.repeat(12)); await input.press('Control+Enter');
  assert.equal(await page.locator('.bubble-unread').count(),0);
  assert.equal(await page.locator('.composer-footer .notification-dot').count(),0);
  await input.press('Escape');
  await page.locator('.bubble-unread').waitFor();
  // The hidden panel remains subscribed, but cannot mark the result as viewed.
  assert.equal(await input.isVisible(),false);
  await bubble.click();
  await page.waitForFunction(()=>!document.querySelector('.bubble-unread'));
  await page.getByRole('button',{name:'运行 trace',exact:true}).click();
  assert.match(await page.locator('.run-trace').textContent(),/demo-sha256:/);
  assert.doesNotMatch(await page.locator('.run-trace').textContent(),/synthetic-secret-not-real/);
  await page.getByText('新输入',{exact:true}).click();
  await input.fill('任务页不代表看过结果 '.repeat(12)); await input.press('Control+Enter');
  await page.getByText('后台运行 ↗',{exact:true}).click();
  await page.locator('.composer-footer .notification-dot').waitFor();
  await page.getByRole('button',{name:'任务：有未查看结果',exact:true}).click();
  await page.locator('.task-row .notification-dot').waitFor();
  assert.equal(await page.locator('.task-row .notification-dot').count(),1);
  await page.getByRole('button',{name:'查看 ↗',exact:true}).first().click();
  await page.getByText('新输入',{exact:true}).waitFor();
  await page.waitForFunction(()=>!document.querySelector('.bubble-unread'));
  await page.getByText('新输入',{exact:true}).click();
  await input.fill('前台完成'); await input.press('Control+Enter');
  await page.getByText('新输入',{exact:true}).waitFor();
  await page.waitForTimeout(1100);
  assert.equal(await page.locator('.bubble-unread').count(),0);
  assert.equal(await page.locator('.composer-footer .quiet').textContent(),'任务');

  // Native bridge UI contract using synthetic IPC; not real WebView/process acceptance.
  const nativePage = await browser.newPage({viewport:{width:520,height:580}});
  nativePage.on('pageerror',error=>errors.push(error.message));
  const config = JSON.parse(await fs.readFile('src-tauri/defaults.json','utf8'));
  await nativePage.addInitScript(config=>{
    const startedAt = Date.now();
    const run = {id:'failed-run',actionId:'normalize-text',actionName:'失败动作',input:'original input',
      status:'failed',stage:'失败',stdout:'partial result',stderr:'sample error',cwd:'C:/fixture',startedAt,
      finishedAt:startedAt+10,error:'进程退出失败，退出码：1',exitCode:1,httpStatus:null,truncated:false,
      submissionUnknown:false,unread:true,outputMode:'live',configRevision:'config-h1',scriptRevision:'source-h1',seq:1,
      traceTruncated:true,trace:[{at:startedAt,step:'插值参数',detail:'["-u", "C:/fixture/versions/source-h1/main.py", "original input"]'},
        {at:startedAt+1,step:'冻结脚本源码',detail:'print("source used by this run")'},
        {at:startedAt+10,step:'执行失败',detail:'exit code 1'}]};
    const callbacks={}, listeners={}; let callbackId=0;
    const state=window.testRuns={run,calls:[],channel:null,failReadAck:false,delayAck:false,finishAck:null,startCount:0,
      emit(name,payload){if(listeners[name])callbacks[listeners[name]]({event:name,id:1,payload});},
      complete(){
        const entry={at:Date.now(),step:'执行失败',detail:'background exit code 1'};
        Object.assign(run,{status:'failed',stage:'失败',unread:true,finishedAt:Date.now(),error:'background failed',exitCode:1,seq:run.seq+1});
        run.trace.push(entry);
        state.channel.onmessage({type:'update',runId:run.id,seq:run.seq,stdoutDelta:'',stderrDelta:'',
          status:run.status,stage:run.stage,error:run.error,exitCode:1,httpStatus:null,finishedAt:run.finishedAt,
          truncated:false,submissionUnknown:false,traceDelta:[entry],traceTruncated:run.traceTruncated});
      }};
    window.isTauri=true;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
    window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'panel'},currentWebview:{label:'panel'}},
      transformCallback(fn){callbacks[++callbackId]=fn;return callbackId;},unregisterCallback(){},
      async invoke(command,args){
        state.calls.push(command);
        switch(command){
          case 'plugin:event|listen':listeners[args.event]=args.handler;return 1;
          case 'plugin:event|unlisten':return;
          case 'load_config':return {config,revision:'config-h1',diskRevision:'config-h1',diskStatus:'valid',directory:'C:/fixture',warning:null};
          case 'selected_action':return 'normalize-text';
          case 'get_theme':return 'light';
          case 'list_runs':return [{...run,preview:run.input}];
          case 'attach_run':state.channel=args.channel;args.channel.onmessage({type:'snapshot',run:structuredClone(run)});return;
          case 'mark_run_read':
            if(state.failReadAck)throw 'history.json: replace: access denied';
            if(state.delayAck)return new Promise(resolve=>{const id=args.runId;state.finishAck=()=>{if(run.id===id)run.unread=false;resolve();};});
            run.unread=false;return;
          case 'detach_run':return;
          case 'resize_panel':return args.height;
          case 'collapse_panel':state.emit('panel-collapsed',null);return;
          case 'start_run':Object.assign(run,{id:'background-run-'+(++state.startCount),status:'running',stage:'执行中',unread:false,error:null,finishedAt:null,seq:0});return run.id;
          default:throw `unexpected IPC ${command}`;
        }
      }};
  },config);
  await nativePage.goto(url+'/?surface=panel');
  await nativePage.waitForFunction(()=>testRuns.calls.filter(command=>command==='plugin:event|listen').length>=3);
  await nativePage.evaluate(()=>testRuns.emit('show-run','failed-run'));
  await nativePage.getByText('进程退出失败，退出码：1',{exact:true}).waitFor();
  assert.equal(await nativePage.evaluate(()=>testRuns.calls.includes('mark_run_read')),false);
  await nativePage.evaluate(()=>{testRuns.failReadAck=true;testRuns.emit('panel-opened',null);});
  await nativePage.getByRole('alert').filter({hasText:'标记结果已查看失败'}).waitFor();
  assert.equal(await nativePage.evaluate(()=>testRuns.run.unread),true);
  await nativePage.evaluate(()=>{testRuns.failReadAck=false;window.dispatchEvent(new Event('focus'));});
  await nativePage.waitForFunction(()=>testRuns.run.unread===false);
  assert.equal(await nativePage.getByRole('alert').count(),0);
  await nativePage.getByRole('button',{name:'运行 trace',exact:true}).click();
  await nativePage.getByText('插值参数',{exact:false}).click();
  assert.match(await nativePage.locator('.run-trace').textContent(),/versions\/source-h1\/main.py/);
  await nativePage.getByText('冻结脚本源码',{exact:false}).click();
  await nativePage.getByText('print("source used by this run")',{exact:true}).waitFor();
  assert.match(await nativePage.locator('.run-trace').textContent(),/部分记录已截断/);
  await fs.mkdir('tmp/run-trace',{recursive:true});
  assert.equal(await nativePage.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await nativePage.screenshot({path:'tmp/run-trace/failed-trace.png'});
  await nativePage.getByText('新输入',{exact:true}).click();
  const nativeInput = nativePage.getByRole('textbox',{name:'输入文本'});
  await nativeInput.fill('background input'); await nativeInput.press('Control+Enter');
  await nativePage.getByText('后台运行 ↗',{exact:true}).waitFor();
  const readCalls = await nativePage.evaluate(()=>testRuns.calls.filter(command=>command==='mark_run_read').length);
  await nativeInput.press('Escape');
  await nativePage.evaluate(()=>testRuns.complete());
  await nativePage.getByText('background failed',{exact:true}).waitFor();
  assert.equal(await nativePage.evaluate(()=>testRuns.calls.filter(command=>command==='mark_run_read').length),readCalls);
  await nativePage.evaluate(()=>testRuns.emit('panel-opened',null));
  await nativePage.waitForFunction(()=>testRuns.run.unread===false);
  // A late acknowledgement for another attachment must not hide the current task's error.
  await nativePage.getByText('新输入',{exact:true}).click();
  await nativeInput.fill('task A'); await nativeInput.press('Control+Enter');
  await nativePage.getByText('后台运行 ↗',{exact:true}).waitFor();
  await nativePage.evaluate(()=>{testRuns.delayAck=true;testRuns.complete();});
  await nativePage.waitForFunction(()=>typeof testRuns.finishAck==='function');
  await nativePage.getByText('新输入',{exact:true}).click();
  await nativeInput.fill('task B'); await nativeInput.press('Control+Enter');
  await nativePage.getByText('后台运行 ↗',{exact:true}).waitFor();
  await nativePage.evaluate(()=>{testRuns.delayAck=false;testRuns.failReadAck=true;testRuns.complete();});
  await nativePage.getByRole('alert').filter({hasText:'标记结果已查看失败'}).waitFor();
  await nativePage.evaluate(()=>testRuns.finishAck());
  await nativePage.getByRole('alert').filter({hasText:'标记结果已查看失败'}).waitFor();
  assert.deepEqual(errors,[]);
  console.log('Run trace UI passed: no task totals, hidden/foreground result acknowledgment, list is not reading, trace source/args/truncation and hidden native subscription. Native IPC is synthetic.');
} finally { await browser.close(); await server.close(); }
