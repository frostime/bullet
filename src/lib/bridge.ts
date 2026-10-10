import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import defaults from '../../src-tauri/defaults.json';
import type { Action, Config, ConfigSnapshot, RunEvent, RunSnapshot, RunSummary, ScriptSnapshot, Theme, TraceEntry } from './types';

export const native = isTauri();
export const demoEvents = new EventTarget();
let demoConfig = loadDemoConfig();
let selected = demoConfig.actions[0]?.id ?? '';
const demoRuns = new Map<string, RunSnapshot>();
const demoSubscriptions = new Map<string, (event: RunEvent) => void>();
const demoScripts = new Map<string, ScriptSnapshot>();
const defaultScript = 'import sys\n\ntext = sys.stdin.read()\nprint(text.strip())\n';

function loadDemoConfig(): Config {
  try { const saved = localStorage.getItem('bullet-demo-config'); if (saved) return JSON.parse(saved); } catch { /* Browser storage can be unavailable. */ }
  return JSON.parse(JSON.stringify(defaults));
}
function revision(value: unknown): string { return JSON.stringify(value); }
async function demoTraceRevision(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return 'demo-sha256:' + Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
}
function demoConfigSnapshot(): ConfigSnapshot {
  return { config: structuredClone(demoConfig), revision: revision(demoConfig), diskRevision: revision(demoConfig), diskStatus: 'valid', directory: '~/.config/bullet (浏览器演示)', warning: null };
}

export async function loadConfig(): Promise<ConfigSnapshot> { return native ? invoke('load_config') : demoConfigSnapshot(); }
// Only native deserialization owns the schema/defaults. This browser projection is for interaction preview.
export async function parseConfig(json: string): Promise<Config> {
  if (native) return invoke('parse_config', { json });
  const value = JSON.parse(json);
  if (!value || typeof value !== 'object' || typeof value.version !== 'number'
    || !value.settings || typeof value.settings !== 'object' || !Array.isArray(value.actions)) {
    throw new Error('JSON 需要 version、settings 对象和 actions 数组');
  }
  return { ...value, settings: { ...defaults.settings, ...value.settings }, actions: value.actions.map((action: Partial<Action>) => {
    if (!action || typeof action.id !== 'string' || typeof action.name !== 'string' || !action.kind || !['cli', 'script', 'http'].includes(action.kind)) {
      throw new Error('每个动作需要 id、name 和有效 kind');
    }
    return { enabled: true, program: '', args: [], cwd: null, env: {}, input: 'stdin', timeoutSec: null,
      scriptExtension: 'py', http: null, ...action,
      output: Object.assign({ mode: 'live', encoding: 'utf-8' }, action.output),
      ...(action.http ? { http: Object.assign({ method: 'POST', headers: {}, body: null }, action.http,
        { response: Object.assign({ format: 'text' }, action.http.response) }) } : {}) };
  }) };
}
export async function restoreConfig(config: Config, diskRevision: string): Promise<ConfigSnapshot> {
  if (native) return invoke('restore_config', { config, diskRevision });
  throw new Error('浏览器演示不读取磁盘，也不提供文件恢复');
}
export async function saveConfig(config: Config, expected: string): Promise<ConfigSnapshot> {
  if (native) return invoke('save_config', { config, revision: expected });
  if (revision(demoConfig) !== expected) throw new Error('配置已变化，请重新载入');
  if (config.version !== 1 || !Array.isArray(config.actions)) throw new Error('配置格式无效');
  const ids = config.actions.map(a => a.id);
  if (new Set(ids).size !== ids.length) throw new Error('动作 ID 重复');
  const next = await parseConfig(JSON.stringify(config));
  localStorage.setItem('bullet-demo-config', JSON.stringify(next)); demoConfig = next; return demoConfigSnapshot();
}
export async function readScript(id: string, extension: string): Promise<ScriptSnapshot> {
  if (native) return invoke('read_script', { id, extension });
  const source = id === 'normalize-text' ? defaultScript : '';
  return demoScripts.get(id + '.' + extension) ?? { source, revision: source };
}
export async function saveScript(id: string, extension: string, source: string, expected: string): Promise<ScriptSnapshot> {
  if (native) return invoke('save_script', { id, extension, source, revision: expected });
  if ((await readScript(id, extension)).revision !== expected) throw new Error('脚本已修改');
  const next = { source, revision: source }; demoScripts.set(id + '.' + extension, next); return next;
}
export async function getTheme(): Promise<Theme> {
  if (native) return invoke('get_theme');
  return localStorage.getItem('bullet-demo-theme') === 'dark' ? 'dark' : 'light';
}
export async function setTheme(theme: Theme): Promise<Theme> {
  if (native) return invoke('set_theme', { theme });
  localStorage.setItem('bullet-demo-theme', theme);
  demoEvents.dispatchEvent(new CustomEvent('theme-changed', { detail: theme }));
  return theme;
}
export async function selectedAction(): Promise<string | null> { return native ? invoke('selected_action') : selected; }
export async function selectAction(id: string): Promise<void> { if (native) await invoke('select_action', { id }); else selected = id; }
export async function listRuns(): Promise<RunSummary[]> {
  if (native) return invoke('list_runs');
  return [...demoRuns.values()].sort((a,b) => b.startedAt - a.startedAt).map(run => ({ ...run, preview: run.input.slice(0,64) }));
}
export async function startRun(actionId: string, text: string): Promise<string> {
  if (native) return invoke('start_run', { actionId, text });
  const action = demoConfig.actions.find(a => a.id === actionId && a.enabled);
  if (!action) throw new Error('请选择有效动作');
  const run: RunSnapshot = { id: crypto.randomUUID(), actionId, actionName: action.name, input: text,
    status: 'running', stage: '执行中', stdout: '', stderr: '', cwd: action.cwd || demoConfig.settings.defaultCwd,
    startedAt: Date.now(), finishedAt: null, error: null, exitCode: null, httpStatus: null,
    truncated: false, submissionUnknown: false, unread: false, outputMode: action.output.mode,
    configRevision: await demoTraceRevision(revision(demoConfig)), scriptRevision: null, seq: 0, traceTruncated: false,
    trace: [{ at: Date.now(), step: '浏览器演示', detail: '仅模拟交互，不代表本机程序、路径解析或真实 HTTP 请求。' }] };
  if (action.kind === 'script') {
    const source = await readScript(action.id, action.scriptExtension);
    run.scriptRevision = await demoTraceRevision(source.source);
    run.trace.push({ at: Date.now(), step: '模拟脚本快照', detail: source.source });
  }
  demoRuns.set(run.id, run);
  const result = actionId === 'show-cwd' ? 'C:\\Users\\you\n'
    : action.kind === 'http' ? '这是 HTTP 返回的演示结果。\n\n' + text
    : text.trim().split('\n').map(line => line.trim()).join('\n');
  const chunkSize = Math.max(14, Math.ceil(result.length / 30));
  const chunks = result.match(new RegExp(`[\\s\\S]{1,${chunkSize}}`, 'g')) ?? [''];
  let index = 0;
  const timer = setInterval(() => {
    if (run.status !== 'running') { clearInterval(timer); return; }
    const delta = chunks[index++] ?? '';
    run.stdout += delta;
    const traceDelta: TraceEntry[] = [];
    if (index >= chunks.length) {
      run.status = 'succeeded'; run.stage = '已完成'; run.finishedAt = Date.now(); run.unread = true; run.exitCode = 0;
      traceDelta.push({ at: Date.now(), step: '模拟完成', detail: '演示结果已返回。' });
      run.trace.push(...traceDelta); clearInterval(timer);
    }
    publishDemo(run, delta, traceDelta);
  }, 260);
  return run.id;
}
function publishDemo(run: RunSnapshot, stdoutDelta = '', traceDelta: TraceEntry[] = []): void {
  run.seq += 1;
  demoSubscriptions.get(run.id)?.({ type: 'update', runId: run.id, seq: run.seq, stdoutDelta, stderrDelta: '',
    status: run.status, stage: run.stage, error: run.error, exitCode: run.exitCode,
    httpStatus: run.httpStatus, finishedAt: run.finishedAt, truncated: false, submissionUnknown: false,
    traceDelta, traceTruncated: run.traceTruncated });
}
export async function attachRun(runId: string, onEvent: (event: RunEvent) => void): Promise<void> {
  if (native) { const channel = new Channel<RunEvent>(); channel.onmessage = onEvent; await invoke('attach_run', { runId, channel }); return; }
  const run = demoRuns.get(runId); if (!run) throw new Error('任务不存在');
  onEvent({ type: 'snapshot', run: structuredClone(run) }); demoSubscriptions.set(runId, onEvent);
}
export async function markRunRead(runId: string): Promise<void> {
  if (native) { await invoke('mark_run_read', { runId }); return; }
  const run = demoRuns.get(runId);
  if (run && run.status !== 'running') run.unread = false;
}
export async function detachRun(runId: string): Promise<void> { if (native) await invoke('detach_run', { runId }); else demoSubscriptions.delete(runId); }
export async function cancelRun(runId: string): Promise<void> {
  if (native) { await invoke('cancel_run', { runId }); return; }
  const run = demoRuns.get(runId); if (run?.status === 'running') { run.status = 'cancelled'; run.stage = '已停止'; run.finishedAt = Date.now(); publishDemo(run); }
}
export async function clearHistory(): Promise<void> {
  if (native) await invoke('clear_history'); else for (const [id,run] of demoRuns) if (run.status !== 'running') demoRuns.delete(id);
}
export async function shell(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  if (native) return invoke(command, args);
  demoEvents.dispatchEvent(new CustomEvent(command, { detail: args })); return args.height;
}
export async function subscribe(name: string, callback: (payload: unknown) => void): Promise<() => void> {
  if (native) return listen(name, event => callback(event.payload));
  const handler = (event: Event) => callback((event as CustomEvent).detail);
  demoEvents.addEventListener(name, handler); return () => demoEvents.removeEventListener(name, handler);
}
export async function showRun(runId: string): Promise<void> {
  if (native) await invoke('show_run', { runId }); else { await shell('toggle_panel', { open: true }); demoEvents.dispatchEvent(new CustomEvent('show-run', { detail: runId })); }
}
export async function resolveCwd(defaultCwd: string, cwd: string | null): Promise<string> {
  if (native) return invoke('resolve_action_cwd', { defaultCwd, cwd });
  return cwd || defaultCwd;
}
export async function confirmAction(message: string): Promise<boolean> {
  if (!native) return window.confirm(message);
  const { confirm } = await import('@tauri-apps/plugin-dialog');
  return confirm(message, { title: 'bullet · 确认操作', kind: 'warning' });
}
export async function chooseDirectory(): Promise<string | null> {
  if (!native) return window.prompt('输入工作目录', 'C:\\Users\\you');
  const { open } = await import('@tauri-apps/plugin-dialog');
  const value = await open({ directory: true, multiple: false, title: '选择工作目录' });
  return typeof value === 'string' ? value : null;
}
export async function startDragging(): Promise<void> { if (native) await getCurrentWindow().startDragging(); }
export function newAction(kind: Action['kind']): Action {
  return { id: 'action-' + crypto.randomUUID().slice(0,8), name: kind === 'script' ? '新脚本' : kind === 'http' ? '新请求' : '新命令',
    kind, enabled: true, program: kind === 'script' ? 'python' : '', args: kind === 'script' ? ['-u', '{{script}}'] : [],
    cwd: null, env: kind === 'script' ? { PYTHONIOENCODING: 'utf-8' } : {}, input: 'stdin', timeoutSec: null,
    output: { mode: 'live', encoding: 'utf-8' }, scriptExtension: 'py',
    http: kind === 'http' ? { url: 'http://127.0.0.1:6806/api/block/appendBlock', method: 'POST', headers: {},
      body: { dataType: 'markdown', data: '{{input}}', parentID: '' }, response: { format: 'json', success: { pointer: '/code', equals: 0 }, successText: '已写入思源' } } : null };
}
