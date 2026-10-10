import { describe, expect, it } from 'vitest';
import { applyRunEvent, hasUnreadResult, type RunSnapshot, type RunEvent } from './types';
const run = { id: 'a', seq: 4, stdout: '旧', stderr: '' } as RunSnapshot;
const update = { type: 'update', runId: 'a', seq: 5, stdoutDelta: '新', stderrDelta: '' } as Extract<RunEvent, {type:'update'}>;
describe('snapshot and delta ordering', () => {
  it('ignores stale updates and other jobs', () => {
    expect(applyRunEvent(run, { ...update, seq: 4 })).toBe(run);
    expect(applyRunEvent(run, { ...update, runId: 'b' })).toBe(run);
  });
  it('retains trace order, ignores duplicate trace deltas, and accepts old snapshots without trace', () => {
    const first = { at: 1, step: '解析目录', detail: 'C:/work' };
    const second = { at: 2, step: '启动进程', detail: 'PID 123' };
    const next = applyRunEvent({ ...run, trace: [first] }, { ...update, traceDelta: [second], traceTruncated: true })!;
    expect(next.trace).toEqual([first, second]);
    expect(next.traceTruncated).toBe(true);
    expect(applyRunEvent(next, { ...update, traceDelta: [second] })).toBe(next);
    expect(applyRunEvent(run, { ...update, traceDelta: [second] })!.trace).toEqual([second]);
  });
  it.each(['running', 'cancelled', 'succeeded', 'failed'] as const)('only unseen results trigger a reminder (%s)', status => {
    expect(hasUnreadResult({ status, unread: true })).toBe(status === 'succeeded' || status === 'failed');
    expect(hasUnreadResult({ status, unread: false })).toBe(false);
  });
  it('appends exactly once and detects gaps', () => {
    const next = applyRunEvent(run, update)!;
    expect(next.stdout).toBe('旧新');
    expect(applyRunEvent(next, update)).toBe(next);
    expect(() => applyRunEvent(run, { ...update, seq: 6 })).toThrow('序号不连续');
  });
});
