<script lang="ts">
  import { onMount } from 'svelte';
  import Panel from './components/Panel.svelte';
  import Settings from './components/Settings.svelte';
  import Tasks from './components/Tasks.svelte';
  import * as bridge from './lib/bridge';
  import { ConfigEditorSession } from './lib/config-editor.svelte';
  const configEditor = new ConfigEditorSession(bridge);
  const surface = new URLSearchParams(location.search).get('surface') ?? 'preview';
  let expanded = $state(false);
  let route = $state(location.hash.slice(1));
  let active = $state(0);
  let bubbleStart: {x:number;y:number} | null = null;
  let dragged = false;
  function navigate(next: string) { route = next; location.hash = next; }
  function setExpanded(value: boolean) {
    expanded = value;
    bridge.demoEvents.dispatchEvent(new CustomEvent(value ? 'panel-opened' : 'panel-collapsed', {detail:{}}));
  }
  function bubbleDown(event: PointerEvent) {
    bubbleStart = { x:event.clientX, y:event.clientY }; dragged = false;
    const element = event.currentTarget as HTMLElement; element.setPointerCapture(event.pointerId);
  }
  function bubbleMove(event: PointerEvent) {
    if (!bubbleStart || !bridge.native || dragged) return;
    if (Math.hypot(event.clientX-bubbleStart.x,event.clientY-bubbleStart.y) > 5) { dragged = true; void bridge.startDragging(); }
  }
  function bubbleUp() {
    bubbleStart = null;
    if (!dragged) { if (bridge.native) void bridge.shell('toggle_panel'); else setExpanded(!expanded); }
  }
  onMount(() => {
    const cleanup: (() => void)[] = []; let disposed = false;
    async function bind(name: string, callback: (payload: unknown) => void) {
      const off = await bridge.subscribe(name, callback); if (disposed) off(); else cleanup.push(off);
    }
    void bind('navigate', payload => navigate(String(payload)));
    if (!bridge.native) {
      void bind('toggle_panel', () => { navigate(''); setExpanded(true); });
      void bind('collapse_panel', () => setExpanded(false));
      void bind('open_management', payload => navigate((payload as {route:string}).route));
    }
    const timer = setInterval(() => { void bridge.listRuns().then(runs => active = runs.filter(run => run.status === 'running').length); }, 1000);
    return () => { disposed = true; cleanup.forEach(off => off()); clearInterval(timer); };
  });
</script>

<svelte:window onhashchange={() => route = location.hash.slice(1)} />
{#snippet bubble()}
  <button class="bubble" aria-label="展开或收起 bullet" title="点击展开 · 拖动移动" onpointerdown={bubbleDown} onpointermove={bubbleMove} onpointerup={bubbleUp} onpointercancel={() => bubbleStart = null} onclick={event => { if (event.detail === 0) { if (bridge.native) void bridge.shell('toggle_panel'); else setExpanded(!expanded); } }}><span class="bullet-mark"></span>{#if active}<span class="bubble-count">{active}</span>{/if}</button>
{/snippet}
{#snippet management()}
  <div class="management-shell">
    <header class="management-nav"><a href="#settings" class="wordmark" onclick={() => navigate('settings')}>bullet<span>●</span></a><nav><button class:current={route !== 'tasks'} onclick={() => navigate('settings')}>动作</button><button class:current={route === 'tasks'} onclick={() => navigate('tasks')}>任务</button></nav><span class="grow"></span>{#if !bridge.native}<button class="quiet" onclick={() => navigate('')}>返回桌面 ↗</button>{/if}</header>
    <main class="management-content">{#if route === 'tasks'}<Tasks />{:else}<Settings session={configEditor} />{/if}</main>
  </div>
{/snippet}

{#if surface === 'bubble'}<div class="native-bubble">{@render bubble()}</div>
{:else if surface === 'panel'}<Panel />
{:else if surface === 'manage'}{@render management()}
{:else}
  <div class="demo-desktop"><header class="demo-intro"><div class="wordmark">bullet<span>●</span></div><h1>一句话，一个动作。</h1><p>点击右下角气泡开始。<br/>Enter 换行，Ctrl + Enter 发送。</p><div class="demo-label">浏览器交互演示 · 执行结果为模拟</div></header>
    <div class="desktop-grid"></div>
    <div class="demo-dock"><div class="demo-panel" class:concealed={!expanded}><Panel /></div>{@render bubble()}</div>
    {#if route === 'settings' || route === 'tasks'}<div class="demo-management">{@render management()}</div>{/if}
    <footer class="demo-footer">TEXT → ACTION<span>CLI / HTTP / SCRIPT</span></footer>
  </div>
{/if}
