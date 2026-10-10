import type { Action, ActionKind, Config, ConfigSnapshot, ScriptSnapshot } from './types';

export interface ConfigEditorPort {
  loadConfig(): Promise<ConfigSnapshot>;
  parseConfig(json: string): Promise<Config>;
  saveConfig(config: Config, revision: string): Promise<ConfigSnapshot>;
  restoreConfig(config: Config, diskRevision: string): Promise<ConfigSnapshot>;
  readScript(id: string, extension: string): Promise<ScriptSnapshot>;
  saveScript(id: string, extension: string, source: string, revision: string): Promise<ScriptSnapshot>;
  newAction(kind: ActionKind): Action;
  confirmAction(message: string): Promise<boolean>;
  chooseDirectory(): Promise<string | null>;
  resolveCwd(defaultCwd: string, cwd: string | null): Promise<string>;
  startRun(id: string, text: string): Promise<string>;
  showRun(id: string): Promise<void>;
}
interface ScriptDraft extends ScriptSnapshot { savedSource: string }
interface FieldBuffers { args: string; env: string; headers: string; body: string }
const jsonText = (value: unknown) => JSON.stringify(value, null, 2);
const cloneConfig = (config: Config): Config => JSON.parse(JSON.stringify(config));

/** One instance per management window. Views may unmount; this editing session does not. */
export class ConfigEditorSession {
  snapshot = $state<ConfigSnapshot | null>(null);
  config = $state<Config | null>(null);
  selectedId = $state('');
  rawMode = $state(false);
  raw = $state('');
  fields = $state<FieldBuffers>({ args: '[]', env: '{}', headers: '{}', body: 'null' });
  scripts = $state<Record<string, ScriptDraft>>({});
  busy = $state('');
  error = $state('');
  message = $state('');
  resolvedCwd = $state('');
  testText = $state('用于试运行的文本');

  constructor(private readonly port: ConfigEditorPort) {}
  get action() { return this.config?.actions.find(item => item.id === this.selectedId); }
  get scriptKey() { return this.action ? `${this.action.id}.${this.action.scriptExtension}` : ''; }
  get script() { return this.action?.kind === 'script' ? this.scripts[this.scriptKey] : undefined; }
  get warning() { return this.snapshot?.diskStatus !== 'valid' ? this.snapshot?.warning ?? '' : ''; }
  get dirty() {
    if (!this.config || !this.snapshot) return false;
    const fieldsChanged = JSON.stringify(this.fields) !== JSON.stringify(this.buffersFor(this.action));
    return JSON.stringify(this.config) !== JSON.stringify(this.snapshot.config)
      || (this.rawMode && this.raw !== jsonText(this.config)) || (!this.rawMode && fieldsChanged)
      || Object.values(this.scripts).some(draft => draft.source !== draft.savedSource);
  }

  async ensureLoaded() {
    if (!this.snapshot) await this.operation('载入配置', () => this.loadDocument());
  }
  async reload() {
    await this.operation('重新载入', async () => {
      if (this.dirty && !await this.port.confirmAction('重新载入会丢弃全部未保存配置和脚本草稿。是否继续？')) return;
      await this.loadDocument();
    });
  }
  async select(id: string) {
    await this.operation('切换动作', async () => {
      this.applyFieldBuffers();
      this.selectedId = id;
      this.fields = this.buffersFor(this.action);
      this.resolvedCwd = '';
      await this.ensureScript();
    });
  }
  async toggleRaw() {
    await this.operation('转换视图', async () => {
      if (this.rawMode) {
        const parsed = await this.port.parseConfig(this.raw);
        this.replaceDraft(parsed);
        this.rawMode = false;
        await this.ensureScript();
      } else {
        this.applyFieldBuffers();
        this.raw = jsonText(this.config);
        this.rawMode = true;
      }
    });
  }
  async add(kind: ActionKind) {
    await this.operation('添加动作', async () => {
      if (!this.config) return;
      this.applyFieldBuffers();
      const next = this.port.newAction(kind);
      // Incomplete actions can be saved while disabled; enable when ready to execute.
      next.enabled = false;
      this.config.actions.push(next);
      this.selectedId = next.id;
      this.fields = this.buffersFor(next);
      await this.ensureScript();
    });
  }
  async remove() {
    await this.operation('移除动作', async () => {
      if (!this.config || !this.action) return;
      if (!await this.port.confirmAction('移除这个动作？磁盘脚本和会话内的源码草稿会保留。')) return;
      this.config.actions = this.config.actions.filter(item => item.id !== this.selectedId);
      this.selectedId = this.config.actions[0]?.id ?? '';
      this.fields = this.buffersFor(this.action);
      await this.ensureScript();
    });
  }
  async changeKind(kind: ActionKind) {
    await this.operation('切换类型', async () => {
      this.applyFieldBuffers();
      const action = this.action;
      if (!action) return;
      action.kind = kind;
      if (kind === 'http' && !action.http) action.http = this.port.newAction('http').http;
      if (kind === 'script' && !action.args.includes('{{script}}')) action.args.push('{{script}}');
      this.fields = this.buffersFor(action);
      this.resolvedCwd = '';
      await this.ensureScript();
    });
  }
  async openScript() {
    await this.operation('载入脚本', () => this.ensureScript());
  }
  async reloadScript() {
    await this.operation('重新载入脚本', async () => {
      if (!this.action || this.action.kind !== 'script') return;
      if (this.script && this.script.source !== this.script.savedSource
        && !await this.port.confirmAction('重新载入会丢弃当前脚本的未保存修改。是否继续？')) return;
      const next = await this.port.readScript(this.action.id, this.action.scriptExtension);
      this.scripts[this.scriptKey] = { ...next, savedSource: next.source };
    });
  }
  async save() {
    return this.operation('保存配置', async () => {
      await this.commitConfig();
      this.message = '配置已保存；脚本源码需单独保存';
    });
  }
  async saveScript() {
    return this.operation('保存脚本', async () => {
      await this.commitScript();
      this.message = '脚本已保存；动作配置未自动保存';
    });
  }
  async restore() {
    return this.operation('恢复配置', async () => {
      if (this.snapshot?.diskStatus !== 'invalid' || !this.snapshot.diskRevision) {
        throw new Error('当前磁盘文件不能恢复，请重新载入以检查文件状态');
      }
      if (!await this.port.confirmAction('将先备份磁盘中的无效配置，再用当前草稿恢复 config.json。脚本源码不会自动保存。是否继续？')) return;
      await this.prepareDraft();
      const next = await this.port.restoreConfig(cloneConfig(this.config!), this.snapshot.diskRevision);
      this.acceptCommit(next);
      this.message = next.warning ?? '配置已恢复；原文件备份保存在配置目录 recovery 中';
    });
  }
  async test() {
    return this.operation('保存并试运行', async () => {
      await this.commitConfig();
      this.message = '配置已保存';
      if (this.action?.kind === 'script') {
        await this.commitScript();
        this.message = '配置和脚本已保存';
      }
      if (!this.action) throw new Error('请先选择一个动作');
      const id = await this.port.startRun(this.action.id, this.testText);
      this.message += '；试运行已启动';
      await this.port.showRun(id);
    });
  }
  async chooseCwd() {
    await this.operation('选择工作目录', async () => {
      const value = await this.port.chooseDirectory();
      if (value && this.action) { this.action.cwd = value; this.resolvedCwd = ''; }
    });
  }
  async resolveCwd() {
    await this.operation('解析工作目录', async () => {
      this.resolvedCwd = '';
      if (this.config && this.action) {
        this.resolvedCwd = await this.port.resolveCwd(this.config.settings.defaultCwd, this.action.cwd ?? null);
      }
    });
  }

  // All async editing operations are serialized. The view disables input while they run.
  private async operation(label: string, work: () => Promise<void>): Promise<boolean> {
    if (this.busy) return false;
    this.busy = label;
    this.error = '';
    this.message = '';
    try { await work(); return true; }
    catch (failure) { this.error = `${label}失败：${String(failure)}`; return false; }
    finally { this.busy = ''; }
  }
  private async loadDocument() {
    const next = await this.port.loadConfig();
    if (this.snapshot) {
      // Refresh the disk observation, but retain the saved edit baseline until all reads succeed.
      this.snapshot = { ...this.snapshot, diskStatus: next.diskStatus, diskRevision: next.diskRevision, warning: next.warning };
      if (next.diskStatus === 'unavailable') {
        throw new Error('磁盘配置无法读取；当前配置和脚本草稿已保留');
      }
    }
    const selected = next.config.actions.find(item => item.id === this.selectedId) ?? next.config.actions[0];
    const scripts: Record<string, ScriptDraft> = {};
    try {
      if (selected?.kind === 'script') {
        const source = await this.port.readScript(selected.id, selected.scriptExtension);
        scripts[`${selected.id}.${selected.scriptExtension}`] = { ...source, savedSource: source.source };
      }
    } catch (failure) {
      if (!this.snapshot) {
        // On first entry there are no drafts to protect; keep configuration editing available.
        this.snapshot = next;
        this.replaceDraft(next.config);
        this.message = '配置已载入，脚本源码未载入';
      }
      throw failure;
    }
    // Replace the session only after the entire reload has succeeded.
    this.snapshot = next;
    this.scripts = scripts;
    this.selectedId = selected?.id ?? '';
    this.replaceDraft(next.config);
    if (this.rawMode) this.raw = jsonText(this.config);
  }
  private replaceDraft(config: Config) {
    this.config = cloneConfig(config);
    if (!this.action) this.selectedId = this.config.actions[0]?.id ?? '';
    this.fields = this.buffersFor(this.action);
    this.resolvedCwd = '';
  }
  private buffersFor(action?: Action): FieldBuffers {
    return { args: jsonText(action?.args ?? []), env: jsonText(action?.env ?? {}),
      headers: jsonText(action?.http?.headers ?? {}), body: jsonText(action?.http?.body ?? null) };
  }
  private applyFieldBuffers() {
    const action = this.action;
    if (!action) return;
    if (action.kind !== 'http') {
      const args = JSON.parse(this.fields.args);
      const env = this.parseStringMap(this.fields.env, '环境变量');
      if (!Array.isArray(args) || args.some(item => typeof item !== 'string')) throw new Error('参数必须是字符串数组');
      action.args = args;
      action.env = env;
    } else if (action.http) {
      const headers = this.parseStringMap(this.fields.headers, '请求头');
      const body = JSON.parse(this.fields.body);
      action.http.headers = headers;
      action.http.body = body;
    }
  }
  private parseStringMap(text: string, label: string): Record<string, string> {
    const value = JSON.parse(text);
    if (!value || Array.isArray(value) || typeof value !== 'object'
      || Object.values(value).some(item => typeof item !== 'string')) throw new Error(`${label}必须是字符串键值对象`);
    return value;
  }
  private async prepareDraft() {
    if (!this.config || !this.snapshot) throw new Error('配置尚未载入');
    if (this.rawMode) this.replaceDraft(await this.port.parseConfig(this.raw));
    else this.applyFieldBuffers();
  }
  private async commitConfig() {
    await this.prepareDraft();
    if (this.snapshot!.diskStatus !== 'valid') {
      throw new Error(this.snapshot!.diskStatus === 'invalid'
        ? '磁盘配置无效，请使用“备份并恢复配置”，不要重复普通保存'
        : '磁盘配置无法读取，请先处理文件错误并重新载入');
    }
    const next = await this.port.saveConfig(cloneConfig(this.config!), this.snapshot!.revision);
    this.acceptCommit(next);
  }
  private acceptCommit(next: ConfigSnapshot) {
    this.snapshot = next;
    this.replaceDraft(next.config);
    this.raw = jsonText(this.config);
  }
  private async ensureScript() {
    if (!this.action || this.action.kind !== 'script' || this.script) return;
    const next = await this.port.readScript(this.action.id, this.action.scriptExtension);
    this.scripts[this.scriptKey] = { ...next, savedSource: next.source };
  }
  private async commitScript() {
    if (!this.action || this.action.kind !== 'script') throw new Error('请选择脚本动作');
    await this.ensureScript();
    const draft = this.script!;
    const next = await this.port.saveScript(this.action.id, this.action.scriptExtension, draft.source, draft.revision);
    this.scripts[this.scriptKey] = { ...next, savedSource: next.source };
  }
}
