<script lang="ts">
  import { onMount, tick } from 'svelte';
  import * as bridge from '../lib/bridge';
  import { applyRunEvent, hasUnreadResult, statusLabels, type Action, type RunSnapshot, type RunSummary } from '../lib/types';
  import RunTrace from './RunTrace.svelte';

  let { visible = true }: { visible?: boolean } = $props();
  let panelOpen = $state(!bridge.native);
  let acknowledgedRun: string | null = null;

  let actions = $state<Action[]>([]);
  let selected = $state('');
  let text = $state('');
  let run = $state<RunSnapshot | null>(null);
  let runs = $state<RunSummary[]>([]);
  let menu = $state(false);
  let submitting = $state(false);
  let error = $state('');
  let readError = $state('');
  let copied = $state(false);
  let logs = $state(false);
  let textarea: HTMLTextAreaElement;
  let resultElement = $state<HTMLDivElement>();
  let followOutput = true;
  let attachmentGeneration = 0;
  let lastHeight = 0;
  let action = $derived(actions.find(value => value.id === selected));
  let unread = $derived(runs.some(hasUnreadResult));
  let outputVisible = $derived(run && (run.outputMode === 'live' || run.status !== 'running'));

  async function refresh() {
    try {
      const snapshot = await bridge.loadConfig(); actions = snapshot.config.actions.filter(value => value.enabled);
      const last = await bridge.selectedAction();
      if (!actions.some(value => value.id === selected)) selected = actions.find(value => value.id === last)?.id ?? actions[0]?.id ?? '';
      if (snapshot.warning) error = snapshot.warning;
      runs = await bridge.listRuns();
    } catch (failure) { error = String(failure); }
  }
  async function attach(id: string) {
    const generation = ++attachmentGeneration;
    acknowledgedRun = null;
    run = null; logs = false; readError = ''; followOutput = true;
    try {
      await bridge.attachRun(id, async event => {
        if (generation !== attachmentGeneration) return;
        try { run = applyRunEvent(run, event); } catch (failure) { error = String(failure); return; }
        if (event.type === 'snapshot') text = event.run.input;
        await tick();
        void acknowledgeVisibleResult();
        if (followOutput && resultElement) resultElement.scrollTop = resultElement.scrollHeight;
      });
    } catch (failure) { error = String(failure); }
  }
  async function acknowledgeVisibleResult() {
    await tick();
    if (!run || (run.status !== 'succeeded' && run.status !== 'failed') || !visible || !panelOpen
      || document.visibilityState !== 'visible' || !document.hasFocus() || acknowledgedRun === run.id) return;
    const id = run.id;
    const generation = attachmentGeneration;
    acknowledgedRun = id;
    try {
      await bridge.markRunRead(id);
    } catch (failure) {
      if (generation !== attachmentGeneration || run?.id !== id) return;
      if (acknowledgedRun === id) acknowledgedRun = null;
      readError = `标记结果已查看失败：${String(failure)}`;
      return;
    }
    if (generation !== attachmentGeneration || run?.id !== id) return;
    readError = '';
    run = { ...run, unread: false };
    try {
      const summaries = await bridge.listRuns();
      if (generation === attachmentGeneration) runs = summaries;
    } catch (failure) {
      if (generation === attachmentGeneration) readError = `结果已标记查看，但任务列表刷新失败：${String(failure)}`;
    }
  }
  async function send() {
    if (submitting || run?.status === 'running' || !selected || !text.trim()) return;
    submitting = true; error = ''; menu = false;
    try { await attach(await bridge.startRun(selected, text)); await refresh(); }
    catch (failure) { error = String(failure); }
    finally { submitting = false; }
  }
  async function detach() {
    if (run) await bridge.detachRun(run.id);
    attachmentGeneration++; run = null; text = ''; error = ''; readError = ''; logs = false; menu = false;
    await tick(); textarea?.focus(); await refresh();
  }
  async function collapse() { menu = false; await bridge.shell('collapse_panel'); }
  function keydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === 'Escape') { event.preventDefault(); if (menu) menu = false; else void collapse(); }
    if (event.key === 'Enter' && event.ctrlKey) { event.preventDefault(); void send(); }
  }
  async function copy() {
    try { await navigator.clipboard.writeText(run?.stdout ?? ''); copied = true; setTimeout(() => copied = false, 1200); }
    catch (failure) { error = `复制失败：${String(failure)}`; }
  }
  function recordScroll() {
    if (resultElement) followOutput = resultElement.scrollHeight - resultElement.scrollTop - resultElement.clientHeight < 24;
  }
  async function resize() {
    await tick();
    if (textarea) { textarea.style.height = 'auto'; textarea.style.height = `${Math.min(Math.max(textarea.scrollHeight, 32), 112)}px`; }
    const height = Math.max(116, (textarea?.offsetHeight ?? 32) + 80 + (run ? 306 + (logs ? 180 : 0) : 0) + (error ? 60 : 0) + (readError ? 60 : 0) + (menu ? Math.min(actions.length * 42 + 58, 258) : 0));
    if (height !== lastHeight) { lastHeight = height; await bridge.shell('resize_panel', { height }); }
  }
  $effect(() => { if (visible) void acknowledgeVisibleResult(); });
  $effect(() => { void text; void run?.status; void logs; void menu; void error; void readError; void actions.length; void resize(); });
  onMount(() => {
    const cleanup: (() => void)[] = [];
    let disposed = false;
    async function bind(name: string, callback: (payload: unknown) => void) {
      const off = await bridge.subscribe(name, callback); if (disposed) off(); else cleanup.push(off);
    }
    void refresh();
    void bind('show-run', payload => { void attach(String(payload)); });
    void bind('panel-opened', () => { panelOpen = true; lastHeight = 0; void refresh(); void resize(); textarea?.focus(); void acknowledgeVisibleResult(); });
    void bind('panel-collapsed', () => { panelOpen = false; menu = false; if (run && run.status !== 'running') void detach(); });
    const timer = setInterval(() => { void bridge.listRuns().then(value => runs = value).catch(() => {}); }, 1000);
    return () => { disposed = true; clearInterval(timer); cleanup.forEach(off => off()); };
  });
</script>

<svelte:window onkeydown={keydown} onfocus={() => void acknowledgeVisibleResult()} />
<svelte:document onvisibilitychange={() => void acknowledgeVisibleResult()} />
<div class="panel-layout">
  <section class="input-card" aria-label="快速输入">
    <div class="composer">
      <textarea bind:this={textarea} bind:value={text} readonly={run?.status === 'running'} rows="1" placeholder="写点什么…" aria-label="输入文本" oninput={() => void resize()}></textarea>
      <button class="send-button" aria-label="发送 Ctrl+Enter" title="Ctrl+Enter 发送" disabled={submitting || run?.status === 'running' || !text.trim() || !selected} onclick={send}>↑</button>
    </div>
    <div class="composer-footer">
      <button class="action-select" aria-expanded={menu} onclick={() => menu = !menu}>{action?.name ?? '配置一个动作'} <span class="chevron">⌄</span></button>
      <div class="quiet-controls">
        <button class="quiet" aria-label={unread ? '任务：有未查看结果' : '任务'} onclick={() => bridge.shell('open_management', {route:'tasks'})}>任务{#if unread}<span class="notification-dot"></span>{/if}</button>
        <button class="icon-button" title="配置动作" aria-label="配置动作" onclick={() => bridge.shell('open_management', {route:'settings'})}>⋯</button>
      </div>
    </div>
    {#if run}
      <div class="result-header"><span class:running={run.status === 'running'} class="status-label">{statusLabels[run.status]}</span><span class="muted">{run.actionName}</span><span class="grow"></span>
        {#if run.status === 'running'}<button class="quiet" onclick={detach}>后台运行 ↗</button><button class="quiet danger" onclick={() => bridge.cancelRun(run!.id)}>停止</button>
        {:else}<button class="quiet" onclick={detach}>新输入</button>{/if}
      </div>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable output needs keyboard access.) -->
      <div class="result-scroll" bind:this={resultElement} onscroll={recordScroll} tabindex="0" role="region" aria-label="执行结果">
        {#if outputVisible && run.stdout}<pre>{run.stdout}</pre>
        {:else if run.status === 'running'}<div class="waiting"><span class="pulse-dot"></span>{run.outputMode === 'buffered' ? '执行中，完成后显示结果' : run.stage}</div>
        {:else}<p class="muted">任务没有返回文本。</p>{/if}
        {#if run.error}<p class="inline-error">{run.error}</p>{/if}
        {#if run.submissionUnknown}<p class="inline-error">未能确认提交结果，请核对目标内容后再决定是否重新执行。</p>{/if}
        {#if run.truncated}<p class="muted">输出超过容量限制，已截断。</p>{/if}
      </div>
      <div class="result-footer"><button class="quiet" disabled={!run.stdout || !outputVisible} onclick={copy}>{copied ? '已复制 ✓' : '复制结果'}</button><button class="quiet" aria-expanded={logs} onclick={() => logs = !logs}>{logs ? '收起 trace' : '运行 trace'}</button><span class="grow"></span><span class="shortcut">Esc 收起</span></div>
      {#if logs}<div class="run-details"><div>cwd · {run.cwd ?? '—'}</div><div>退出码 · {run.exitCode ?? '—'}　HTTP · {run.httpStatus ?? '—'}</div><RunTrace {run} /><details><summary>stderr 日志</summary><pre>{run.stderr || '没有日志'}</pre></details></div>{/if}
    {/if}
    {#if error}<p class="panel-error" role="alert">{error}</p>{/if}
    {#if readError}<p class="panel-error" role="alert">{readError}</p>{/if}
  </section>
  {#if menu}
    <div class="action-menu" role="group" aria-label="选择动作">
      <div class="action-menu-scroll">{#each actions as option}<button class:selected={selected === option.id} onclick={async () => { selected = option.id; menu = false; await bridge.selectAction(selected); }}>{option.name}<span>{selected === option.id ? '✓' : option.kind.toUpperCase()}</span></button>{/each}</div>
      <button class="menu-settings" onclick={() => { menu = false; void bridge.shell('open_management', { route:'settings' }); }}>管理动作 ↗</button>
    </div>
  {/if}
</div>
