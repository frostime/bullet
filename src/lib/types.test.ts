import { describe, expect, it } from 'vitest';
import { applyRunEvent, type RunSnapshot, type RunEvent } from './types';
const run = { id: 'a', seq: 4, stdout: '旧', stderr: '' } as RunSnapshot;
const update = { type: 'update', runId: 'a', seq: 5, stdoutDelta: '新', stderrDelta: '' } as Extract<RunEvent, {type:'update'}>;
describe('snapshot and delta ordering', () => {
  it('ignores stale updates and other jobs', () => {
    expect(applyRunEvent(run, { ...update, seq: 4 })).toBe(run);
    expect(applyRunEvent(run, { ...update, runId: 'b' })).toBe(run);
  });
  it('appends exactly once and detects gaps', () => {
    const next = applyRunEvent(run, update)!;
    expect(next.stdout).toBe('旧新');
    expect(applyRunEvent(next, update)).toBe(next);
    expect(() => applyRunEvent(run, { ...update, seq: 6 })).toThrow('序号不连续');
  });
});
