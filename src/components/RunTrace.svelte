<script lang="ts">
  import type { RunSnapshot } from '../lib/types';
  let { run }: { run: RunSnapshot } = $props();
  const stepLabels: Record<string, string> = {
    accepted: '接受任务', cwd: '解析工作目录', program: '查找运行程序', script: '冻结脚本版本与源码',
    args: '插值参数', env: '配置环境变量覆盖', spawn: '启动进程', stdin: '输入方式与 stdin EOF',
    exit: '进程退出', http_request: '准备 HTTP 请求', http_headers: '展开请求头', http_body: '插值请求体',
    http_response: '收到 HTTP 响应', json_parse: '解析返回 JSON', condition: '检查成功条件',
    done_marker: '识别流式完成标记', terminal: '运行结束',
  };
</script>

<section class="run-trace" aria-label="当次运行 trace">
  <p class="muted">当次执行记录 · 已识别的鉴权字段脱敏 · 不包含脚本内部逐行执行</p>
  <div>配置版本 · {run.configRevision}</div>
  {#if run.scriptRevision}<div>脚本版本 · {run.scriptRevision}</div>{/if}
  {#if run.trace?.length}
    <ol>
      {#each run.trace as entry}
        <li><details>
          <summary><span class="trace-time">+{Math.max(0, entry.at - run.startedAt)} ms</span> {stepLabels[entry.step] ?? entry.step}</summary>
          <pre>{entry.detail}</pre>
        </details></li>
      {/each}
    </ol>
  {:else}<p class="muted">这条历史记录没有 trace。旧记录不能用当前配置重建。</p>{/if}
  {#if run.traceTruncated}<p class="inline-error">Trace 超过容量限制，部分记录已截断。执行和输出读取仍继续。</p>{/if}
</section>
