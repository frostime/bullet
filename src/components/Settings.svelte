<script lang="ts">
  import { onMount } from 'svelte';
  import * as bridge from '../lib/bridge';
  import type { ActionKind } from '../lib/types';
  import type { ConfigEditorSession } from '../lib/config-editor.svelte';
  let { session }: { session: ConfigEditorSession } = $props();
  let config = $derived(session.config);
  let action = $derived(session.action);
  let script = $derived(session.script);
  async function changeKind(event: Event & { currentTarget: HTMLSelectElement }) {
    const select = event.currentTarget;
    await session.changeKind(select.value as ActionKind);
    select.value = session.action?.kind ?? '';
  }
  onMount(() => { void session.ensureLoaded(); });
</script>

<div class="page-heading"><div><p class="eyebrow">CONFIGURATION</p><h1>配置动作</h1><p class="muted">在这里配置好，日常只输入文本。</p></div><button class="secondary" onclick={() => bridge.shell('open_config_directory')}>打开配置目录 ↗</button></div>
{#if session.error}<p class="inline-error" role="alert">{session.error}</p>{/if}
{#if session.warning}<p class="config-warning" role="status">{session.warning}</p>{/if}
{#if !config}<button class="secondary" disabled={!!session.busy} onclick={() => session.ensureLoaded()}>载入配置</button>{/if}
{#if config}
  <fieldset class="settings-session" disabled={!!session.busy}>
  <div class="settings-toolbar"><button class="quiet" onclick={() => session.reload()}>重新载入</button><button class="quiet" onclick={() => session.toggleRaw()}>{session.rawMode ? '表单视图' : '编辑完整 JSON'}</button><span class="muted small">{session.dirty ? '有未保存修改' : '无未保存修改'}</span><span class="grow"></span><span class="save-message" role="status">{session.message}</span><button class="primary" onclick={() => session.save()}>{session.busy || '保存配置'}</button></div>
  {#if session.snapshot?.diskStatus === 'invalid' && session.snapshot.diskRevision}
    <div class="config-recovery"><p>磁盘配置无效，当前显示上一份有效配置。恢复前会备份原文件；不会自动保存脚本。</p><button class="secondary" onclick={() => session.restore()}>备份并恢复配置</button></div>
  {/if}
  {#if session.rawMode}
    <textarea class="code-editor full-config" bind:value={session.raw} spellcheck="false" aria-label="完整配置 JSON"></textarea>
    <p class="muted small">切回表单会解析当前 JSON，不会写入磁盘。解析失败时保留输入。脚本源码单独保存。</p>
  {:else}
    <section class="preferences"><label>默认工作目录<input bind:value={config.settings.defaultCwd} placeholder="~" /></label><label>系统通知<select bind:value={config.settings.notifications}><option value="background">后台任务完成或失败</option><option value="all">全部任务</option><option value="off">关闭</option></select></label><label>保留结果<input type="number" min="1" max="200" bind:value={config.settings.historyLimit} /></label></section>
    <div class="action-workspace">
      <aside class="actions-sidebar"><h2>动作 <span class="muted">{config.actions.length}</span></h2>
        {#each config.actions as item}<button class:selected={session.selectedId === item.id} onclick={() => session.select(item.id)}><span>{item.name}</span><small>{item.kind.toUpperCase()}{item.enabled ? '' : ' · 停用'}</small></button>{/each}
        <div class="add-actions"><button onclick={() => session.add('cli')}>＋ CLI</button><button onclick={() => session.add('http')}>＋ HTTP</button><button onclick={() => session.add('script')}>＋ Script</button></div>
      </aside>
      <section class="action-editor">
        {#if action}
          <div class="editor-heading"><h2>{action.name}</h2><button class="quiet danger" onclick={() => session.remove()}>移除动作</button></div>
          <div class="field-grid"><label>名称<input bind:value={action.name} /></label><label>类型<select aria-label="动作类型" value={action.kind} onchange={changeKind}><option value="cli">CLI</option><option value="http">HTTP</option><option value="script">托管 Script</option></select></label></div>
          <div class="enabled-row"><label class="checkbox-label"><input type="checkbox" bind:checked={action.enabled} />启用动作</label><span class="muted small">ID · {action.id}</span></div>
          <p class="field-hint">新动作默认停用，可先保存未完成的配置；准备好后启用。</p>
          {#if action.kind !== 'http'}
            <label>运行程序<input bind:value={action.program} placeholder="python / uv / pi / powershell.exe" /></label>
            <label>参数数组<textarea class="code-editor compact" bind:value={session.fields.args} spellcheck="false"></textarea></label>
            <p class="field-hint">{'{{input}}'} 作为一个参数传入；{'{{script}}'} 替换为托管脚本的绝对路径。</p>
            <label>工作目录<div class="input-with-button"><input bind:value={action.cwd} placeholder={`继承默认目录：${config.settings.defaultCwd}`} /><button class="secondary" onclick={() => session.chooseCwd()}>选择</button><button class="secondary" onclick={() => session.resolveCwd()}>解析</button></div></label>
            {#if session.resolvedCwd}<p class="resolved-path">{session.resolvedCwd}</p>{/if}
            <div class="field-grid"><label>输入方式<select bind:value={action.input}><option value="stdin">stdin（写入后关闭）</option><option value="args">参数中的 {'{{input}}'}</option></select></label><label>编码<input bind:value={action.output.encoding} placeholder="utf-8 / gbk" /></label></div>
            <details><summary>环境变量</summary><textarea class="code-editor compact" bind:value={session.fields.env} spellcheck="false" aria-label="环境变量 JSON"></textarea></details>
            {#if action.kind === 'script'}
              <div class="script-heading"><h3>托管脚本</h3><label>扩展名<input class="extension-input" bind:value={action.scriptExtension} onchange={() => session.openScript()} /></label><button class="quiet" onclick={() => session.reloadScript()}>重新载入脚本</button><button class="secondary" onclick={() => session.saveScript()}>保存脚本</button></div>
              {#if script}<textarea class="code-editor source-editor" bind:value={script.source} spellcheck="false" aria-label="脚本源码"></textarea>
              {:else}<p class="muted small">源码尚未载入。<button class="quiet" onclick={() => session.openScript()}>载入脚本</button></p>{/if}
              <p class="field-hint">切换动作或扩展名保留源码草稿；退出应用不保留未保存修改。</p>
              <p class="field-hint">保存到 ~/.config/bullet/scripts/{action.id}/main.{action.scriptExtension}。执行使用源码版本快照，cwd 独立配置。</p>
            {/if}
          {:else if action.http}
            <div class="field-grid http-address"><label>请求地址<input bind:value={action.http.url} /></label><label>方法<select bind:value={action.http.method}><option>POST</option><option>GET</option><option>PUT</option><option>PATCH</option><option>DELETE</option></select></label></div>
            <label>请求头<textarea class="code-editor compact" bind:value={session.fields.headers} spellcheck="false"></textarea></label>
            <p class="field-hint">可使用 {'${env:VARIABLE}'} 引用系统环境变量。</p>
            <label>JSON 请求体<textarea class="code-editor body-editor" bind:value={session.fields.body} spellcheck="false"></textarea></label>
            <p class="field-hint">字符串中的 {'{{input}}'} 替换为输入文本，自动处理引号和换行。</p>
            <div class="field-grid"><label>响应协议<select bind:value={action.http.response.format}><option value="text">Text</option><option value="json">JSON</option><option value="sse">SSE</option><option value="jsonl">JSONL</option></select></label><label>结果 JSON Pointer<input bind:value={action.http.response.pointer} placeholder="/choices/0/message/content" /></label></div>
            {#if ['sse','jsonl'].includes(action.http.response.format)}<label>完成标记<input bind:value={action.http.response.doneMarker} placeholder="例如 [DONE]；留空按连接结束" /></label>{/if}
            <p class="field-hint">业务成功条件、成功回执等字段可在完整 JSON 中设置。</p>
          {/if}
          <div class="field-grid"><label>结果显示<select bind:value={action.output.mode}><option value="live">逐步显示</option><option value="buffered">执行完成后显示</option></select></label><label>超时（秒）<input type="number" min="1" bind:value={action.timeoutSec} placeholder="留空不限时" /></label></div>
          <div class="test-area"><label>试运行输入<textarea rows="2" bind:value={session.testText}></textarea></label><button class="secondary" onclick={() => session.test()}>保存并试运行 ↗</button></div>
        {:else}<div class="empty-row">添加一个动作开始配置。</div>{/if}
      </section>
    </div>
  {/if}
  <p class="config-location muted small">{session.snapshot?.directory}/config.json</p>
  </fieldset>
{/if}
