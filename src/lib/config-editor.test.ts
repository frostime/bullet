import { describe, expect, it, vi } from 'vitest';
import defaults from '../../src-tauri/defaults.json';
import { ConfigEditorSession, type ConfigEditorPort } from './config-editor.svelte';
import type { Action, Config, ConfigSnapshot, ScriptSnapshot } from './types';

const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value));
function fixture() {
  let persisted = copy(defaults) as Config;
  const sources = new Map<string, ScriptSnapshot>([
    ['normalize-text.py', { source: 'old Python', revision: 'py-1' }],
    ['normalize-text.js', { source: 'old JavaScript', revision: 'js-1' }],
  ]);
  const snapshot = (): ConfigSnapshot => ({ config: copy(persisted), revision: JSON.stringify(persisted),
    diskRevision: JSON.stringify(persisted), diskStatus: 'valid', directory: '/fixture', warning: null });
  const port: ConfigEditorPort = {
    loadConfig: vi.fn(async () => snapshot()),
    parseConfig: vi.fn(async json => {
      const parsed = JSON.parse(json);
      if (!Array.isArray(parsed.actions)) throw new Error('actions 必须为数组');
      return parsed as Config;
    }),
    saveConfig: vi.fn(async (config, revision) => {
      if (revision !== JSON.stringify(persisted)) throw new Error('版本冲突');
      persisted = copy(config);
      return snapshot();
    }),
    restoreConfig: vi.fn(async config => { persisted = copy(config); return snapshot(); }),
    readScript: vi.fn(async (id, extension) => copy(sources.get(`${id}.${extension}`) ?? { source: '', revision: '' })),
    saveScript: vi.fn(async (id, extension, source, revision) => {
      const key = `${id}.${extension}`;
      if ((sources.get(key)?.revision ?? '') !== revision) throw new Error('脚本冲突');
      const next = { source, revision: source };
      sources.set(key, next);
      return next;
    }),
    newAction: kind => ({ ...copy(defaults.actions[0]), id: 'new-action', name: '新动作', kind } as Action),
    confirmAction: vi.fn(async () => true),
    chooseDirectory: async () => null,
    resolveCwd: async () => '/fixture',
    startRun: vi.fn(async () => 'run-1'),
    showRun: vi.fn(async () => {}),
  };
  return { session: new ConfigEditorSession(port), port, persisted: () => copy(persisted), snapshot };
}

describe('configuration editing session', () => {
  it('keeps saved JSON arguments and env when returning to the form and saving again', async () => {
    const { session, persisted } = fixture();
    await session.ensureLoaded();
    await session.toggleRaw();
    const edited = JSON.parse(session.raw);
    edited.actions[0].args = ['-B', '{{script}}'];
    edited.actions[0].env = { EXAMPLE: 'new value' };
    session.raw = JSON.stringify(edited);
    expect(await session.save()).toBe(true);
    await session.toggleRaw();
    expect(JSON.parse(session.fields.args)).toEqual(['-B', '{{script}}']);
    expect(JSON.parse(session.fields.env)).toEqual({ EXAMPLE: 'new value' });
    await session.save();
    expect(persisted().actions[0].args).toEqual(['-B', '{{script}}']);
    expect(persisted().actions[0].env).toEqual({ EXAMPLE: 'new value' });
  });

  it('applies unsaved JSON on view switch and keeps invalid text without switching or saving', async () => {
    const { session, port } = fixture();
    await session.ensureLoaded();
    await session.toggleRaw();
    const edited = JSON.parse(session.raw);
    edited.actions[0].name = '未保存的新名称';
    session.raw = JSON.stringify(edited);
    await session.toggleRaw();
    expect(session.action?.name).toBe('未保存的新名称');
    expect(session.dirty).toBe(true);
    expect(port.saveConfig).not.toHaveBeenCalled();
    await session.toggleRaw();
    session.raw = '{unfinished';
    await session.toggleRaw();
    expect(session.rawMode).toBe(true);
    expect(session.raw).toBe('{unfinished');
    expect(session.error).toContain('转换视图失败');
    expect(await session.save()).toBe(false);
    expect(port.saveConfig).not.toHaveBeenCalled();
  });

  it('retains source drafts by action and extension, with separate script and config commits', async () => {
    const { session, port } = fixture();
    await session.ensureLoaded();
    session.script!.source = 'new Python';
    await session.select('show-cwd');
    await session.select('normalize-text');
    expect(session.script!.source).toBe('new Python');
    session.action!.scriptExtension = 'js';
    await session.openScript();
    session.script!.source = 'new JavaScript';
    session.action!.scriptExtension = 'py';
    await session.openScript();
    expect(session.script!.source).toBe('new Python');
    await session.save();
    expect(port.saveScript).not.toHaveBeenCalled();
    expect(session.dirty).toBe(true);
    await session.saveScript();
    expect(session.script!.savedSource).toBe('new Python');
    session.action!.scriptExtension = 'js';
    expect(session.script!.source).toBe('new JavaScript');
  });

  it('keeps drafts when reload is cancelled or cannot read the document', async () => {
    const { session, port } = fixture();
    await session.ensureLoaded();
    session.action!.name = 'keep me';
    session.script!.source = 'keep source';
    port.confirmAction = vi.fn(async () => false);
    await session.reload();
    expect(session.action!.name).toBe('keep me');
    expect(session.script!.source).toBe('keep source');
    port.confirmAction = vi.fn(async () => true);
    port.loadConfig = vi.fn(async () => { throw new Error('read failure'); });
    await session.reload();
    expect(session.action!.name).toBe('keep me');
    expect(session.script!.source).toBe('keep source');
    expect(session.error).toContain('read failure');
  });

  it('preserves every draft when native load returns unavailable or a script cannot be read', async () => {
    const { session, port, snapshot } = fixture();
    await session.ensureLoaded();
    session.action!.name = 'keep configuration';
    session.script!.source = 'keep script';
    const originalRevision = session.snapshot!.revision;
    port.loadConfig = vi.fn(async () => ({ ...snapshot(), diskStatus: 'unavailable' as const,
      diskRevision: null, warning: 'cannot read config.json' }));
    await session.reload();
    expect(session.action!.name).toBe('keep configuration');
    expect(session.script!.source).toBe('keep script');
    expect(session.snapshot!.revision).toBe(originalRevision);
    expect(session.snapshot!.diskStatus).toBe('unavailable');
    expect(session.warning).toBe('cannot read config.json');
    port.loadConfig = vi.fn(async () => snapshot());
    port.readScript = vi.fn(async () => { throw new Error('脚本必须为 UTF-8'); });
    await session.reload();
    expect(session.action!.name).toBe('keep configuration');
    expect(session.script!.source).toBe('keep script');
    expect(session.error).toContain('UTF-8');
    expect(session.snapshot!.revision).toBe(originalRevision);
    expect(session.snapshot!.diskStatus).toBe('valid');
    expect(session.warning).toBe('');
  });

  it('serializes saving and reload, retaining the returned commit as the next baseline', async () => {
    const { session, port, snapshot } = fixture();
    await session.ensureLoaded();
    session.action!.name = 'saved name';
    let complete!: (value: ConfigSnapshot) => void;
    port.saveConfig = vi.fn(() => new Promise<ConfigSnapshot>(resolve => { complete = resolve; }));
    const saving = session.save();
    await vi.waitFor(() => expect(port.saveConfig).toHaveBeenCalledOnce());
    await session.reload();
    await session.select('show-cwd');
    expect(port.loadConfig).toHaveBeenCalledTimes(1);
    expect(session.selectedId).toBe('normalize-text');
    const next = snapshot();
    next.config.actions[0].name = 'saved name';
    next.revision = next.diskRevision = 'new-revision';
    complete(next);
    expect(await saving).toBe(true);
    expect(session.action!.name).toBe('saved name');
    expect(session.snapshot!.revision).toBe('new-revision');
    expect(session.dirty).toBe(false);
  });

  it('requires explicit confirmation and disk revision for invalid-file recovery', async () => {
    const { session, port, snapshot } = fixture();
    port.loadConfig = vi.fn(async () => ({ ...snapshot(), diskRevision: 'broken-hash', diskStatus: 'invalid' as const, warning: 'using fallback' }));
    await session.ensureLoaded();
    expect(await session.save()).toBe(false);
    expect(port.saveConfig).not.toHaveBeenCalled();
    port.confirmAction = vi.fn(async () => false);
    await session.restore();
    expect(port.restoreConfig).not.toHaveBeenCalled();
    port.confirmAction = vi.fn(async () => true);
    await session.restore();
    expect(port.restoreConfig).toHaveBeenCalledWith(expect.anything(), 'broken-hash');
    expect(session.snapshot!.diskStatus).toBe('valid');
    expect(session.warning).toBe('');
  });

  it('reports a partial commit and never runs after a script save failure', async () => {
    const { session, port, persisted } = fixture();
    await session.ensureLoaded();
    session.action!.name = 'saved config';
    session.script!.source = 'unsaved source';
    port.saveScript = vi.fn(async () => { throw new Error('脚本冲突'); });
    expect(await session.test()).toBe(false);
    expect(persisted().actions[0].name).toBe('saved config');
    expect(session.script!.source).toBe('unsaved source');
    expect(session.message).toBe('配置已保存');
    expect(session.error).toContain('脚本冲突');
    expect(port.startRun).not.toHaveBeenCalled();
  });

  it('adds incomplete disabled actions and preserves common fields on type changes', async () => {
    const { session } = fixture();
    await session.ensureLoaded();
    session.action!.cwd = '/work';
    session.action!.timeoutSec = 47;
    session.action!.enabled = false;
    await session.changeKind('http');
    expect(session.action!.cwd).toBe('/work');
    expect(session.action!.timeoutSec).toBe(47);
    expect(session.action!.enabled).toBe(false);
    await session.changeKind('script');
    expect(session.action!.program).toBe('python');
    expect(session.action!.args).toContain('{{script}}');
    await session.add('cli');
    expect(session.action!.enabled).toBe(false);
  });
});
