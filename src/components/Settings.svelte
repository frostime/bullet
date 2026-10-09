<script lang="ts">
  import { onMount } from 'svelte';
  import * as bridge from '../lib/bridge';
  import type { Action, ActionKind, Config, ConfigSnapshot } from '../lib/types';
  let snapshot = $state<ConfigSnapshot | null>(null);
  let config = $state<Config | null>(null);
  let selectedId = $state('');
  let rawMode = $state(false);
  let raw = $state('');
  let argsText = $state('[]');
  let envText = $state('{}');
  let headersText = $state('{}');
  let bodyText = $state('{}');
  let source = $state('');
  let scriptRevision = $state('');
  let scriptKey = $state('');
  let resolvedCwd = $state('');
  let message = $state('');
  let error = $state('');
  let saving = $state(false);
  let testText = $state('用于试运行的文本');
  let action = $derived(config?.actions.find(item => item.id === selectedId));
  let loadGeneration = 0;

  async function load() {
    try {
      snapshot = await bridge.loadConfig(); config = JSON.parse(JSON.stringify(snapshot.config));
      raw = JSON.stringify(config, null, 2);
      await select(config!.actions.find(item => item.id === selectedId) ?? config!.actions[0]);
      error = snapshot.warning ?? ''; message = '';
    } catch (failure) { error = String(failure); }
  }
  async function select(next?: Action) {
    ++loadGeneration;
    selectedId = next?.id ?? ''; error = ''; message = ''; resolvedCwd = '';
    argsText = JSON.stringify(next?.args ?? [], null, 2); envText = JSON.stringify(next?.env ?? {}, null, 2);
    headersText = JSON.stringify(next?.http?.headers ?? {}, null, 2); bodyText = JSON.stringify(next?.http?.body ?? {}, null, 2);
    source = ''; scriptRevision = ''; scriptKey = '';
    if (next?.kind === 'script') await loadSource(next);
  }
  async function loadSource(next: Action) {
    const generation = ++loadGeneration;
    try {
      const script = await bridge.readScript(next.id, next.scriptExtension);
      if (generation !== loadGeneration) return;
      source = script.source; scriptRevision = script.revision; scriptKey = `${next.id}.${next.scriptExtension}`;
    } catch (failure) { error = String(failure); }
  }
  function syncFields() {
    if (!action) return;
    if (action.kind !== 'http') {
      const args = JSON.parse(argsText); const env = JSON.parse(envText);
      if (!Array.isArray(args) || args.some(item => typeof item !== 'string')) throw new Error('参数必须是字符串数组');
      if (!env || Array.isArray(env) || typeof env !== 'object' || Object.values(env).some(value => typeof value !== 'string')) throw new Error('环境变量必须是字符串键值对象');
      action.args = args; action.env = env;
    } else if (action.http) {
      const headers = JSON.parse(headersText);
      if (!headers || Array.isArray(headers) || typeof headers !== 'object' || Object.values(headers).some(value => typeof value !== 'string')) throw new Error('请求头必须是字符串键值对象');
      action.http.headers = headers; action.http.body = JSON.parse(bodyText);
      if (!action.http.response.doneMarker) action.http.response.doneMarker = null;
      if (action.http.response.format !== 'json') {
        action.http.response.success = null;
        action.http.response.successText = null;
      }
    }
  }
  async function save() {
    if (!snapshot || !config) return;
    saving = true; error = ''; message = '';
    try {
      if (rawMode) config = JSON.parse(raw); else syncFields();
      snapshot = await bridge.saveConfig(config!, snapshot.revision); config = JSON.parse(JSON.stringify(snapshot.config));
      raw = JSON.stringify(config, null, 2); message = '配置已保存';
    } catch (failure) { error = String(failure); }
    finally { saving = false; }
  }
  async function saveSource() {
    if (!action || action.kind !== 'script') return;
    error = ''; message = '';
    try {
      if (scriptKey !== `${action.id}.${action.scriptExtension}`) throw new Error('扩展名已变化，请先重新载入脚本');
      const next = await bridge.saveScript(action.id, action.scriptExtension, source, scriptRevision);
      scriptRevision = next.revision; message = '脚本已保存';
    } catch (failure) { error = String(failure); }
  }
  async function add(kind: ActionKind) {
    if (!config) return;
    try { syncFields(); const next = bridge.newAction(kind); config.actions.push(next); await select(next); }
    catch (failure) { error = String(failure); }
  }
  async function changeKind(kind: ActionKind) {
    if (!action) return;
    const fresh = bridge.newAction(kind);
    Object.assign(action, fresh, { id: action.id, name: action.name }); await select(action);
  }
  async function remove() {
    if (!config || !action) return;
    config.actions = config.actions.filter(value => value.id !== selectedId); await select(config.actions[0]);
  }
  async function chooseCwd() { try { const value = await bridge.chooseDirectory(); if (value && action) action.cwd = value; } catch (failure) { error = String(failure); } }
  async function resolve() {
    try { if (config && action) resolvedCwd = await bridge.resolveCwd(config.settings.defaultCwd, action.cwd ?? null); }
    catch (failure) { error = String(failure); }
  }
  async function test() {
    await save(); if (error || !action) return;
    if (action.kind === 'script') { await saveSource(); if (error) return; }
    try { const id = await bridge.startRun(action.id, testText); await bridge.showRun(id); }
    catch (failure) { error = String(failure); }
  }
  function toggleRaw() {
    try { if (!rawMode) { syncFields(); raw = JSON.stringify(config, null, 2); } rawMode = !rawMode; }
    catch (failure) { error = String(failure); }
  }
  onMount(() => { void load(); });
</script>

<div class="page-heading"><div><p class="eyebrow">CONFIGURATION</p><h1>配置动作</h1><p class="muted">在这里配置好，日常只输入文本。</p></div><button class="secondary" onclick={() => bridge.shell('open_config_directory')}>打开配置目录 ↗</button></div>
{#if config}
  <div class="settings-toolbar"><button class="quiet" onclick={load}>重新载入</button><button class="quiet" onclick={toggleRaw}>{rawMode ? '表单视图' : '编辑完整 JSON'}</button><span class="grow"></span><span class="save-message" role="status">{message}</span><button class="primary" disabled={saving} onclick={save}>{saving ? '保存中…' : '保存配置'}</button></div>
  {#if error}<p class="inline-error" role="alert">{error}</p>{/if}
  {#if rawMode}
    <textarea class="code-editor full-config" bind:value={raw} spellcheck="false" aria-label="完整配置 JSON"></textarea>
    <p class="muted small">返回表单前，请先保存 JSON。脚本源码单独保存。</p>
  {:else}
    <section class="preferences"><label>默认工作目录<input bind:value={config.settings.defaultCwd} placeholder="~" /></label><label>系统通知<select bind:value={config.settings.notifications}><option value="background">后台任务完成或失败</option><option value="all">全部任务</option><option value="off">关闭</option></select></label><label>保留结果<input type="number" min="1" max="200" bind:value={config.settings.historyLimit} /></label></section>
    <div class="action-workspace">
      <aside class="actions-sidebar"><h2>动作 <span class="muted">{config.actions.length}</span></h2>
        {#each config.actions as item}<button class:selected={selectedId === item.id} onclick={() => { try { syncFields(); void select(item); } catch (failure) { error = String(failure); } }}><span>{item.name}</span><small>{item.kind.toUpperCase()}{item.enabled ? '' : ' · 停用'}</small></button>{/each}
        <div class="add-actions"><button onclick={() => add('cli')}>＋ CLI</button><button onclick={() => add('http')}>＋ HTTP</button><button onclick={() => add('script')}>＋ Script</button></div>
      </aside>
      <section class="action-editor">
        {#if action}
          <div class="editor-heading"><h2>{action.name}</h2><button class="quiet danger" onclick={remove}>移除动作</button></div>
          <div class="field-grid"><label>名称<input bind:value={action.name} /></label><label>类型<select value={action.kind} onchange={event => changeKind(event.currentTarget.value as ActionKind)}><option value="cli">CLI</option><option value="http">HTTP</option><option value="script">托管 Script</option></select></label></div>
          <div class="enabled-row"><label class="checkbox-label"><input type="checkbox" bind:checked={action.enabled} />启用动作</label><span class="muted small">ID · {action.id}</span></div>
          {#if action.kind !== 'http'}
            <label>运行程序<input bind:value={action.program} placeholder="python / uv / pi / powershell.exe" /></label>
            <label>参数数组<textarea class="code-editor compact" bind:value={argsText} spellcheck="false"></textarea></label>
            <p class="field-hint">{'{{input}}'} 作为一个参数传入；{'{{script}}'} 替换为托管脚本的绝对路径。</p>
            <label>工作目录<div class="input-with-button"><input bind:value={action.cwd} placeholder={`继承默认目录：${config.settings.defaultCwd}`} /><button class="secondary" onclick={chooseCwd}>选择</button><button class="secondary" onclick={resolve}>解析</button></div></label>
            {#if resolvedCwd}<p class="resolved-path">{resolvedCwd}</p>{/if}
            <div class="field-grid"><label>输入方式<select bind:value={action.input}><option value="stdin">stdin（写入后关闭）</option><option value="args">参数中的 {'{{input}}'}</option></select></label><label>编码<input bind:value={action.output.encoding} placeholder="utf-8 / gbk" /></label></div>
            <details><summary>环境变量</summary><textarea class="code-editor compact" bind:value={envText} spellcheck="false" aria-label="环境变量 JSON"></textarea></details>
            {#if action.kind === 'script'}
              <div class="script-heading"><h3>托管脚本</h3><label>扩展名<input class="extension-input" bind:value={action.scriptExtension} /></label><button class="quiet" onclick={() => loadSource(action!)}>载入</button><button class="secondary" onclick={saveSource}>保存脚本</button></div>
              <textarea class="code-editor source-editor" bind:value={source} spellcheck="false" aria-label="脚本源码"></textarea>
              <p class="field-hint">保存到 ~/.config/bullet/scripts/{action.id}/main.{action.scriptExtension}。执行使用源码版本快照，cwd 独立配置。</p>
            {/if}
          {:else if action.http}
            <div class="field-grid http-address"><label>请求地址<input bind:value={action.http.url} /></label><label>方法<select bind:value={action.http.method}><option>POST</option><option>GET</option><option>PUT</option><option>PATCH</option><option>DELETE</option></select></label></div>
            <label>请求头<textarea class="code-editor compact" bind:value={headersText} spellcheck="false"></textarea></label>
            <p class="field-hint">可使用 {'${env:VARIABLE}'} 引用系统环境变量。</p>
            <label>JSON 请求体<textarea class="code-editor body-editor" bind:value={bodyText} spellcheck="false"></textarea></label>
            <p class="field-hint">字符串中的 {'{{input}}'} 替换为输入文本，自动处理引号和换行。</p>
            <div class="field-grid"><label>响应协议<select bind:value={action.http.response.format}><option value="text">Text</option><option value="json">JSON</option><option value="sse">SSE</option><option value="jsonl">JSONL</option></select></label><label>结果 JSON Pointer<input bind:value={action.http.response.pointer} placeholder="/choices/0/message/content" /></label></div>
            {#if ['sse','jsonl'].includes(action.http.response.format)}<label>完成标记<input bind:value={action.http.response.doneMarker} placeholder="例如 [DONE]；留空按连接结束" /></label>{/if}
            <p class="field-hint">业务成功条件、成功回执等字段可在完整 JSON 中设置。</p>
          {/if}
          <div class="field-grid"><label>结果显示<select bind:value={action.output.mode}><option value="live">逐步显示</option><option value="buffered">执行完成后显示</option></select></label><label>超时（秒）<input type="number" min="1" bind:value={action.timeoutSec} placeholder="留空不限时" /></label></div>
          <div class="test-area"><label>试运行输入<textarea rows="2" bind:value={testText}></textarea></label><button class="secondary" onclick={test}>保存并试运行 ↗</button></div>
        {:else}<div class="empty-row">添加一个动作开始配置。</div>{/if}
      </section>
    </div>
  {/if}
  <p class="config-location muted small">{snapshot?.directory}/config.json</p>
{/if}
