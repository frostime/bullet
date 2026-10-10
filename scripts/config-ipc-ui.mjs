import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createServer} from 'vite';
import {chromium} from 'playwright';
const fixture=JSON.parse(await fs.readFile('src-tauri/defaults.json','utf8'));
const server=await createServer({server:{host:'127.0.0.1',port:1433,strictPort:false}});await server.listen();
const browser=await chromium.launch({headless:true,executablePath:process.env.BULLET_TEST_CHROMIUM || undefined});
try{
 const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript(config=>{
  const copy=value=>JSON.parse(JSON.stringify(value));
  window.isTauri=true;
  const state=window.testIPC={calls:[],confirm:false,delaySave:false,failScript:false,failReadScript:false,complete:null,
   snapshot:{config,revision:'valid-h1',diskRevision:'invalid-h2',diskStatus:'invalid',directory:'C:/fixture',warning:'磁盘配置无效，正在使用上一份有效配置'}};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
  window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'manage'},currentWebview:{label:'manage'}},transformCallback(){return 1;},unregisterCallback(){},
   async invoke(command,args){
    state.calls.push({command,args:copy(args??{})});
    switch(command){
     case 'plugin:event|listen':return 1;
     case 'plugin:event|unlisten':return;
     case 'plugin:dialog|message':return state.confirm?'Ok':'Cancel';
     case 'get_theme':return 'light';
     case 'list_runs':return [];
     case 'load_config':return copy(state.snapshot);
     case 'parse_config':return JSON.parse(args.json);
     case 'read_script':if(state.failReadScript)throw '脚本必须为 UTF-8';return {source:'old source',revision:'script-h1'};
     case 'restore_config':
      if(args.diskRevision!==state.snapshot.diskRevision)throw '磁盘版本变化';
      state.snapshot={...state.snapshot,config:copy(args.config),revision:'restored-h3',diskRevision:'restored-h3',diskStatus:'valid',warning:'配置已恢复；原文件备份：C:/fixture/recovery/invalid-h2.json'};
      return copy(state.snapshot);
     case 'save_config':{
      const commit=()=>{state.snapshot={...state.snapshot,config:copy(args.config),revision:'saved-h4',diskRevision:'saved-h4',diskStatus:'valid',warning:null};return copy(state.snapshot);};
      if(state.delaySave)return new Promise(resolve=>{state.complete=()=>resolve(commit());});
      return commit();
     }
     case 'save_script':if(state.failScript)throw 'C:/fixture/scripts/normalize-text/main.py：replace：拒绝访问 (os error 5)';return {source:args.source,revision:'script-h2'};
     case 'start_run':throw '工作目录不存在：C:/missing';
     default:throw `unexpected command ${command}`;
    }
   }
  };
 },fixture);
 await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/?surface=manage#settings`);
 await page.getByRole('textbox',{name:'脚本源码'}).fill('new draft');
 await page.getByRole('button',{name:'保存配置',exact:true}).click();
 assert.match(await page.getByRole('alert').textContent(),/磁盘配置无效/);
 assert.equal(await page.evaluate(()=>testIPC.calls.some(call=>call.command==='save_config')),false);
 await page.getByRole('button',{name:'备份并恢复配置',exact:true}).click();
 assert.equal(await page.evaluate(()=>testIPC.calls.some(call=>call.command==='restore_config')),false);
 await page.getByLabel('名称',{exact:true}).fill('recovered draft');await page.evaluate(()=>testIPC.confirm=true);
 await page.getByRole('button',{name:'备份并恢复配置',exact:true}).click();
 await page.getByText('配置已恢复；原文件备份：C:/fixture/recovery/invalid-h2.json',{exact:true}).waitFor();
 const recovery=await page.evaluate(()=>testIPC.calls.find(call=>call.command==='restore_config').args);
 assert.equal(recovery.diskRevision,'invalid-h2');assert.equal(recovery.config.actions[0].name,'recovered draft');
 assert.equal(await page.getByRole('button',{name:'备份并恢复配置',exact:true}).count(),0);
 assert.equal(await page.getByRole('textbox',{name:'脚本源码'}).inputValue(),'new draft');
 await page.evaluate(()=>testIPC.delaySave=true);
 await page.getByRole('button',{name:'保存配置',exact:true}).click();
 assert.equal(await page.getByLabel('名称',{exact:true}).isDisabled(),true);
 assert.equal(await page.getByRole('button',{name:'重新载入',exact:true}).isDisabled(),true);
 await page.evaluate(()=>{testIPC.complete();testIPC.delaySave=false;});
 await page.getByText('配置已保存；脚本源码需单独保存',{exact:true}).waitFor();
 assert.equal(await page.getByLabel('名称',{exact:true}).isEnabled(),true);
 // Native load can return an unavailable fallback rather than rejecting the call.
 await page.getByLabel('名称',{exact:true}).fill('keep after failed reload');
 await page.evaluate(()=>{testIPC.snapshot.diskStatus='unavailable';testIPC.snapshot.diskRevision=null;testIPC.snapshot.warning='cannot read config.json';});
 await page.getByRole('button',{name:'重新载入',exact:true}).click();
 await page.getByRole('alert').filter({hasText:'草稿已保留'}).waitFor();
 assert.equal(await page.getByLabel('名称',{exact:true}).inputValue(),'keep after failed reload');
 assert.equal(await page.getByRole('textbox',{name:'脚本源码'}).inputValue(),'new draft');
 await page.evaluate(()=>{testIPC.snapshot.diskStatus='valid';testIPC.snapshot.diskRevision='saved-h4';testIPC.snapshot.warning=null;testIPC.failReadScript=true;});
 await page.getByRole('button',{name:'重新载入',exact:true}).click();
 await page.getByRole('alert').filter({hasText:'UTF-8'}).waitFor();
 assert.equal(await page.locator('.config-warning').count(),0);
 assert.equal(await page.getByLabel('名称',{exact:true}).inputValue(),'keep after failed reload');
 assert.equal(await page.getByRole('textbox',{name:'脚本源码'}).inputValue(),'new draft');
 await page.evaluate(()=>testIPC.failReadScript=false);
 await page.getByRole('button',{name:'重新载入',exact:true}).click();
 await page.getByRole('textbox',{name:'脚本源码'}).fill('new draft');
 await page.evaluate(()=>testIPC.failScript=true);
 await page.getByRole('button',{name:'保存并试运行 ↗',exact:true}).click();
 await page.getByRole('alert').filter({hasText:'replace'}).waitFor();
 assert.equal(await page.locator('.save-message').textContent(),'配置已保存');
 assert.equal(await page.evaluate(()=>testIPC.calls.some(call=>call.command==='start_run')),false);
 assert.equal(await page.getByRole('textbox',{name:'脚本源码'}).inputValue(),'new draft');
 await page.evaluate(()=>testIPC.failScript=false);
 await page.getByRole('button',{name:'保存并试运行 ↗',exact:true}).click();
 await page.getByRole('alert').filter({hasText:'工作目录不存在'}).waitFor();
 assert.equal(await page.locator('.save-message').textContent(),'配置和脚本已保存');
 assert.deepEqual(errors,[]);
 console.log('PASS: native bridge contract with fake IPC: explicit recovery/cancel/diskRevision, busy input lock, unavailable/script-read reload preservation, partial commits and launch failures. Not native runtime acceptance.');
}finally{await browser.close();await server.close();}
