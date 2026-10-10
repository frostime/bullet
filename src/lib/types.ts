export type Theme = 'light' | 'dark';
export type RunStatus = 'running' | 'succeeded' | 'failed' | 'cancelled';
export type ActionKind = 'cli' | 'script' | 'http';
export type OutputMode = 'live' | 'buffered';
export interface HttpConfig {
  url: string; method: string; headers: Record<string, string>; body: unknown;
  response: { format: 'text' | 'json' | 'sse' | 'jsonl'; pointer?: string | null;
    doneMarker?: string | null; success?: { pointer: string; equals: unknown } | null; successText?: string | null };
}
export interface Action {
  id: string; name: string; kind: ActionKind; enabled: boolean; program: string; args: string[];
  cwd?: string | null; env: Record<string, string>; input: 'stdin' | 'args'; timeoutSec?: number | null;
  output: { mode: OutputMode; encoding: string }; scriptExtension: string; http?: HttpConfig | null;
}
export interface Config {
  version: number;
  settings: { defaultCwd: string; notifications: 'background' | 'all' | 'off'; historyLimit: number; maxOutputBytes: number };
  actions: Action[];
}
export interface ConfigSnapshot {
  config: Config;
  /** Revision of the last valid configuration, not necessarily the current disk bytes. */
  revision: string;
  diskRevision: string | null;
  diskStatus: 'valid' | 'invalid' | 'unavailable';
  directory: string;
  warning: string | null;
}
export interface ScriptSnapshot { source: string; revision: string }
export interface TraceEntry { at: number; step: string; detail: string }
export interface RunSnapshot {
  id: string; actionId: string; actionName: string; input: string; status: RunStatus; stage: string;
  stdout: string; stderr: string; cwd: string | null; startedAt: number; finishedAt: number | null;
  error: string | null; exitCode: number | null; httpStatus: number | null; truncated: boolean;
  submissionUnknown: boolean; unread: boolean; outputMode: OutputMode; configRevision: string;
  scriptRevision: string | null; seq: number; trace: TraceEntry[]; traceTruncated: boolean;
}
export interface RunSummary {
  id: string; actionName: string; status: RunStatus; stage: string; startedAt: number;
  finishedAt: number | null; unread: boolean; preview: string;
}
export type RunEvent = { type: 'snapshot'; run: RunSnapshot } | {
  type: 'update'; runId: string; seq: number; stdoutDelta: string; stderrDelta: string;
  status: RunStatus; stage: string; error: string | null; exitCode: number | null; httpStatus: number | null;
  finishedAt: number | null; truncated: boolean; submissionUnknown: boolean;
  traceDelta: TraceEntry[]; traceTruncated: boolean;
};

export function applyRunEvent(current: RunSnapshot | null, event: RunEvent): RunSnapshot | null {
  if (event.type === 'snapshot') return event.run;
  if (!current || current.id !== event.runId || event.seq <= current.seq) return current;
  if (event.seq !== current.seq + 1) throw new Error('输出序号不连续，请重新打开任务');
  return { ...current, stdout: current.stdout + event.stdoutDelta, stderr: current.stderr + event.stderrDelta,
    seq: event.seq, status: event.status, stage: event.stage, error: event.error,
    exitCode: event.exitCode, httpStatus: event.httpStatus, finishedAt: event.finishedAt,
    truncated: event.truncated, submissionUnknown: event.submissionUnknown,
    trace: [...(current.trace ?? []), ...(event.traceDelta ?? [])], traceTruncated: event.traceTruncated ?? current.traceTruncated };
}
export function hasUnreadResult(run: Pick<RunSummary, 'status' | 'unread'>): boolean {
  return run.unread && (run.status === 'succeeded' || run.status === 'failed');
}
export const statusLabels: Record<RunStatus, string> = { running: '执行中', succeeded: '已完成', failed: '失败', cancelled: '已停止' };
