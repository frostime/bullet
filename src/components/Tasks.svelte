<script lang="ts">
  import { onMount } from 'svelte';
  import * as bridge from '../lib/bridge';
  import { statusLabels, type RunSummary } from '../lib/types';
  let runs = $state<RunSummary[]>([]);
  let error = $state('');
  let active = $derived(runs.filter(run => run.status === 'running'));
  let finished = $derived(runs.filter(run => run.status !== 'running'));
  async function refresh() { try { runs = await bridge.listRuns(); } catch (failure) { error = String(failure); } }
  async function clear() { await bridge.clearHistory(); await refresh(); }
  onMount(() => { void refresh(); const timer = setInterval(refresh, 1000); return () => clearInterval(timer); });
</script>
<div class="page-heading"><div><p class="eyebrow">ACTIVITY</p><h1>任务与结果</h1><p class="muted">前台与后台，共用同一份执行状态。</p></div><button class="secondary" onclick={clear}>清空已完成记录</button></div>
{#if error}<p class="inline-error">{error}</p>{/if}
{#each [{label:'运行中',items:active}, {label:'最近完成',items:finished}] as section}
  <section class="task-section"><h2>{section.label}<span class="muted"> {section.items.length}</span></h2>
    {#if !section.items.length}<div class="empty-row">{section.label === '运行中' ? '现在没有运行中的任务。' : '发送一次任务，结果会留在这里。'}</div>{/if}
    {#each section.items as run}
      <div class="task-row"><div class="task-identity"><div><strong>{run.actionName}</strong>{#if run.unread}<span class="notification-dot"></span>{/if}<span class="status-pill" class:running={run.status === 'running'}>{statusLabels[run.status]}</span></div><p>{run.preview || '空输入'}</p><span class="muted small">{new Date(run.startedAt).toLocaleString()}</span></div><div class="row-actions">{#if run.status === 'running'}<button class="quiet danger" onclick={() => bridge.cancelRun(run.id)}>停止</button>{/if}<button class="secondary" onclick={() => bridge.showRun(run.id)}>查看 ↗</button></div></div>
    {/each}
  </section>
{/each}
