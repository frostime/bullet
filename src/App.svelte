<script lang="ts">
  import { onMount } from 'svelte';
  import Panel from './components/Panel.svelte';
  import Settings from './components/Settings.svelte';
  import Tasks from './components/Tasks.svelte';
  import * as bridge from './lib/bridge';
  import { hasUnreadResult } from './lib/types';
  import { ConfigEditorSession } from './lib/config-editor.svelte';
  import { ThemeSession } from './lib/theme.svelte';
  const theme = new ThemeSession();
  const configEditor = new ConfigEditorSession(bridge);
  const surface = new URLSearchParams(location.search).get('surface') ?? 'preview';
  let expanded = $state(false);
  let route = $state(location.hash.slice(1));
  let unread = $state(false);
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
    const cleanup: (() => void)[] = [theme.connect()]; let disposed = false;
    async function bind(name: string, callback: (payload: unknown) => void) {
      const off = await bridge.subscribe(name, callback); if (disposed) off(); else cleanup.push(off);
    }
    void bind('navigate', payload => navigate(String(payload)));
    if (!bridge.native) {
      void bind('toggle_panel', () => { navigate(''); setExpanded(true); });
      void bind('collapse_panel', () => setExpanded(false));
      void bind('open_management', payload => navigate((payload as {route:string}).route));
    }
    async function refreshUnread() {
      try { unread = (await bridge.listRuns()).some(hasUnreadResult); } catch { /* Keep the last known indicator until the next poll. */ }
    }
    void refreshUnread();
    const timer = setInterval(refreshUnread, 1000);
    return () => { disposed = true; cleanup.forEach(off => off()); clearInterval(timer); };
  });
</script>

<svelte:window onhashchange={() => route = location.hash.slice(1)} />
{#snippet bubble()}
  <button class="bubble" aria-label="展开或收起 bullet" title="点击展开 · 拖动移动" onpointerdown={bubbleDown} onpointermove={bubbleMove} onpointerup={bubbleUp} onpointercancel={() => bubbleStart = null} onclick={event => { if (event.detail === 0) { if (bridge.native) void bridge.shell('toggle_panel'); else setExpanded(!expanded); } }}><span class="logo-mark" aria-hidden="true"></span>{#if unread}<span class="bubble-unread" aria-label="有未查看结果"></span>{/if}</button>
{/snippet}
{#snippet management()}
  <div class="management-shell">
    <header class="management-nav">
      <a href="#settings" class="wordmark" onclick={() => navigate('settings')}><span class="logo-mark" aria-hidden="true"></span>bullet</a>
      <nav><button class:current={route !== 'tasks'} onclick={() => navigate('settings')}>动作</button><button class:current={route === 'tasks'} onclick={() => navigate('tasks')}>任务</button></nav>
      <span class="grow"></span>
      {#if !bridge.native}<button class="quiet" onclick={() => navigate('')}>返回桌面 ↗</button>{/if}
      <button class="theme-toggle" aria-label={theme.current === 'light' ? '切换到暗色主题' : '切换到亮色主题'} title={theme.current === 'light' ? '切换到暗色主题' : '切换到亮色主题'} disabled={!theme.ready || theme.busy} onclick={() => theme.toggle()}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          {#if theme.current === 'light'}<path d="M20.5 14a8.5 8.5 0 0 1-10.5-10.5A8.5 8.5 0 1 0 20.5 14Z" />
          {:else}<circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M4.93 4.93l1.42 1.42m11.3 11.3 1.42 1.42M4.93 19.07l1.42-1.42m11.3-11.3 1.42-1.42" />{/if}
        </svg>
      </button>
    </header>
    {#if theme.error}<div class="inline-error" role="alert">主题设置失败：{theme.error}</div>{/if}
    <main class="management-content">{#if route === 'tasks'}<Tasks />{:else}<Settings session={configEditor} />{/if}</main>
  </div>
{/snippet}

{#if surface === 'bubble'}<div class="native-bubble">{@render bubble()}</div>
{:else if surface === 'panel'}<Panel />
{:else if surface === 'manage'}{@render management()}
{:else}
  <div class="demo-desktop"><header class="demo-intro"><div class="wordmark"><span class="logo-mark" aria-hidden="true"></span>bullet</div><h1>一句话，一个动作。</h1><p>点击右下角气泡开始。<br/>Enter 换行，Ctrl + Enter 发送。</p><div class="demo-label">浏览器交互演示 · 执行结果为模拟</div></header>
    <div class="desktop-grid"></div>
    <div class="demo-dock"><div class="demo-panel" class:concealed={!expanded}><Panel visible={expanded && route !== 'settings' && route !== 'tasks'} /></div>{@render bubble()}</div>
    {#if route === 'settings' || route === 'tasks'}<div class="demo-management">{@render management()}</div>{/if}
    <footer class="demo-footer">TEXT → ACTION<span>CLI / HTTP / SCRIPT</span></footer>
  </div>
{/if}
