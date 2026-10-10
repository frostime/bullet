//! 运行 trace 与未读确认的验收测试。
//! CLI 用例执行隔离的 cmd/PowerShell shim；HTTP 用例使用本地 TCP 回环服务器。

use bullet_core::{
    config::{model::ActionKind, ConfigStore},
    execution::{RunEvent, RunManager, RunSnapshot, RunStatus, TraceEntry},
};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn harness() -> (tempfile::TempDir, Arc<ConfigStore>, Arc<RunManager>) {
    let root = tempfile::tempdir().unwrap();
    let store =
        Arc::new(ConfigStore::open(root.path().to_path_buf(), root.path().to_path_buf()).unwrap());
    let manager = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    (root, store, manager)
}

/// 记录文本版本脚本 + Windows 垫片程序，供 CLI/脚本 trace 用例复用。
#[cfg(windows)]
fn shim_harness() -> (tempfile::TempDir, Arc<ConfigStore>, Arc<RunManager>) {
    let root = tempfile::Builder::new()
        .prefix("bullet trace test ")
        .tempdir()
        .unwrap();
    let bin = root.path().join("configured bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(
        bin.join("bullet-trace-cli.cmd"),
        concat!(
            "@echo off\r\n",
            "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" ",
            "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"%~dp0echo.ps1\" %*\r\n",
        ),
    )
    .unwrap();
    std::fs::write(
        bin.join("echo.ps1"),
        concat!(
            "$utf8 = New-Object System.Text.UTF8Encoding($false)\r\n",
            "[Console]::InputEncoding = $utf8\r\n[Console]::OutputEncoding = $utf8\r\n",
            "[Console]::Out.Write((ConvertTo-Json -InputObject @($args) -Compress))\r\n",
            "Start-Sleep -Milliseconds 250\r\n",
        ),
    )
    .unwrap();
    let store =
        Arc::new(ConfigStore::open(root.path().to_path_buf(), root.path().to_path_buf()).unwrap());
    let (store, bin) = (store, bin);
    let mut config = store.load();
    let action = &mut config.config.actions[0];
    action.kind = ActionKind::Cli;
    action.program = "bullet-trace-cli".into();
    action.args = vec!["{{script}}".into(), "extra-arg".into()];
    action
        .env
        .insert("Path".into(), bin.to_string_lossy().into_owned());
    store.save(config.config, &config.revision).unwrap();
    let runs = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    (root, store, runs)
}

async fn wait_for(manager: &RunManager, id: &str) -> RunStatus {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let status = manager
                .list()
                .into_iter()
                .find(|run| run.id == id)
                .unwrap()
                .status;
            if status != RunStatus::Running {
                break status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

/// 结束后订阅一次，快照事件包含完整 trace；返回最终快照。
async fn final_snapshot(manager: &RunManager, id: &str) -> RunSnapshot {
    wait_for(manager, id).await;
    let snapshot = Arc::new(Mutex::new(None));
    let received = snapshot.clone();
    manager
        .subscribe(
            id,
            Arc::new(move |event| {
                if let RunEvent::Snapshot { run } = event {
                    *received.lock().unwrap() = Some(run);
                }
                true
            }),
        )
        .unwrap();
    let run = snapshot.lock().unwrap().clone().unwrap();
    run
}

/// trace 的总字节数（step + detail，UTF-8 字节）。
fn trace_bytes(trace: &[TraceEntry]) -> usize {
    trace
        .iter()
        .map(|entry| entry.step.len() + entry.detail.len())
        .sum()
}

fn trace_text(snapshot: &bullet_core::execution::RunSnapshot) -> String {
    snapshot
        .trace
        .iter()
        .map(|entry| format!("{}: {}", entry.step, entry.detail))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 工作目录不存在：提交应得 runId，稍后成为带 trace 的失败终态并进入历史。
#[tokio::test]
async fn preparation_failure_becomes_failed_run_with_trace_and_history() {
    let (_root, store, manager) = harness();
    let mut config = store.load();
    config.config.actions[0].cwd = Some("Z:/肯定不存在的目录/bullet".into());
    store.save(config.config, &config.revision).unwrap();
    let id = manager.start("normalize-text", "输入内容".into()).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Failed);
    let run = final_snapshot(&manager, &id).await;
    assert!(run.error.as_deref().unwrap().contains("工作目录不存在"));
    let text = trace_text(&run);
    assert!(text.contains("accepted"), "{text}");
    assert!(text.contains("cwd: 解析失败"));
    assert!(text.contains("terminal: 失败"), "{text}");
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.root.join("history.json")).unwrap()).unwrap();
    let saved = history
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(saved["status"], "failed");
    assert!(saved["trace"].as_array().unwrap().len() >= 2);
}

/// 解析类 trace 必须落在冻结版本上；编辑脚本后前一条 trace 不被改写，新 run 用新版本。
#[cfg(windows)]
#[tokio::test]
async fn resolution_trace_keeps_frozen_source_and_arrays_immutable() {
    let (root, store, manager) = shim_harness();
    let mut config = store.load();
    config.config.actions[0].kind = ActionKind::Script;
    store.save(config.config, &config.revision).unwrap();
    let script = store.read_script("normalize-text", "py").unwrap();
    let first_source = "print('你好，冻结版本一')";
    store
        .save_script("normalize-text", "py", first_source, &script.revision)
        .unwrap();
    let first_id = manager.start("normalize-text", "".into()).unwrap();
    // start 返回后立即订阅：cwd、脚本冻结版本和 trace 必须已经齐备，不靠后续 Update 补发。
    let early = Arc::new(Mutex::new(None));
    {
        let received = early.clone();
        manager
            .subscribe(
                &first_id,
                Arc::new(move |event| {
                    if let RunEvent::Snapshot { run } = event {
                        *received.lock().unwrap() = Some(run);
                    }
                    true
                }),
            )
            .unwrap();
    }
    let early_run = early.lock().unwrap().clone().unwrap();
    assert!(early_run.cwd.is_some(), "start 返回后快照必须有 cwd");
    assert_eq!(
        early_run.script_revision,
        Some(bullet_core::config::fingerprint(first_source.as_bytes())),
        "start 返回后快照必须有冻结版本"
    );
    assert!(
        trace_text(&early_run).contains("script: 版本 "),
        "{}",
        trace_text(&early_run)
    );
    // 不等终态，立即编辑脚本：正在运行的 run 仍锁定在冻结版本上。
    let current = store.read_script("normalize-text", "py").unwrap();
    let second_source = "print('你好，冻结版本二')";
    store
        .save_script("normalize-text", "py", second_source, &current.revision)
        .unwrap();

    assert_eq!(wait_for(&manager, &first_id).await, RunStatus::Succeeded);
    let first_run = final_snapshot(&manager, &first_id).await;
    let first_text = trace_text(&first_run);
    assert!(first_text.contains("script: 版本 "), "{first_text}");
    assert!(first_text.contains(first_source), "{first_text}");
    assert!(!first_text.contains(second_source), "{first_text}");
    assert!(
        first_text.contains(r#"原始 ["{{script}}","extra-arg"]"#),
        "{first_text}"
    );
    let first_version = first_run.script_revision.clone().unwrap();
    assert_eq!(
        first_version,
        bullet_core::config::fingerprint(first_source.as_bytes())
    );

    let first_again = final_snapshot(&manager, &first_id).await;
    let again_text = trace_text(&first_again);
    assert!(again_text.contains(first_source), "{again_text}");
    assert!(!again_text.contains(second_source));
    assert_eq!(first_again.script_revision.unwrap(), first_version);

    let second_id = manager.start("normalize-text", "".into()).unwrap();
    assert_eq!(wait_for(&manager, &second_id).await, RunStatus::Succeeded);
    let second_run = final_snapshot(&manager, &second_id).await;
    let second_text = trace_text(&second_run);
    assert!(second_text.contains(second_source), "{second_text}");
    assert!(!second_text.contains(first_source), "{second_text}");
    assert_ne!(second_run.script_revision.unwrap(), first_version);
    // 冻结版本目录确实分叉：目录名即版本号
    let versions = root.path().join("scripts/normalize-text/versions");
    assert_eq!(std::fs::read_dir(&versions).unwrap().count(), 2);
}

// These fixtures receive reqwest JSON POSTs with Content-Length, not chunked bodies.
async fn read_json_post(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 16_384];
    loop {
        let size = socket.read(&mut chunk).await.unwrap();
        assert_ne!(size, 0, "request closed before the declared body arrived");
        request.extend_from_slice(&chunk[..size]);
        if let Some(head_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let length = String::from_utf8_lossy(&request[..head_end])
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("content-length:"))
                .and_then(|line| line.split_once(':'))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .expect("JSON POST fixture requires Content-Length");
            if request.len() >= head_end + 4 + length {
                return request;
            }
        }
    }
}

/// HTTP trace 记录方法/URL/渲染后请求体/头摘要；命名的密钥值（.env 展开与敏感头）不落 trace。
#[tokio::test]
async fn http_trace_redacts_auth_and_env_headers_and_records_body() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_json_post(&mut socket).await;
        assert!(String::from_utf8_lossy(&request).contains(r#""text":"hello 输入""#));
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"ok\":true,\"x\":0}")
            .await
            .unwrap();
    });
    let (_root, store, manager) = harness();
    let mut snapshot = store.load();
    snapshot.config.actions = vec![serde_json::from_value(serde_json::json!({
        "id":"redact","name":"Redact","kind":"http","http": {
            "url": format!("http://{address}/api"),
            "method": "POST",
            "headers": {
                "Authorization": "Bearer ${env:BULLET_TRACE_SECRET}",
                "X-Tenant": "${env:BULLET_TRACE_TENANT}",
                "X-Plain": "plain-ok"
            },
            "body": {"text": "{{input}}"},
            "response": {"format":"json","pointer":"/x","success":{"pointer":"/ok","equals":true}}
        }
    }))
    .unwrap()];
    store.save(snapshot.config, &snapshot.revision).unwrap();
    let secret = "super-secret-已隐藏值-123";
    std::env::set_var("BULLET_TRACE_SECRET", secret);
    std::env::set_var("BULLET_TRACE_TENANT", "tenant-九");
    let id = manager.start("redact", "hello 输入".into()).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Succeeded);
    let run = final_snapshot(&manager, &id).await;
    let text = trace_text(&run);
    assert!(
        text.contains("http_request") && text.contains(format!("http://{address}/api").as_str()),
        "{text}"
    );
    assert!(text.contains(r#""text":"hello 输入""#), "{text}");
    assert!(text.contains("X-Plain: plain-ok"), "{text}");
    assert!(text.contains("Authorization: （已隐藏）"), "{text}");
    assert!(text.contains("X-Tenant: （环境变量值已隐藏）"), "{text}");
    assert!(
        text.contains("http_response") && text.contains("HTTP 200"),
        "{text}"
    );
    assert!(text.contains("json_parse"), "{text}");
    assert!(
        text.contains("condition") && text.contains("满足"),
        "{text}"
    );
    assert!(text.contains("terminal: 完成"), "{text}");
    assert!(!text.contains(secret), "密钥泄漏进 trace：{text}");
    // 持久化历史里也不得出现解析后的密钥。
    let history = std::fs::read_to_string(store.root.join("history.json")).unwrap();
    assert!(!history.contains(secret));
    server.await.unwrap();
}

/// 超大的请求体在写入 trace 前就地 UTF-8 安全限幅；执行照常完成并保留终态记录。
#[tokio::test]
async fn oversized_trace_content_is_bounded_but_run_completes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // Drain the declared body before responding so the close cannot reset the sender.
        read_json_post(&mut socket).await;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .await
            .unwrap();
    });
    let (_root, store, manager) = harness();
    let mut snapshot = store.load();
    snapshot.config.actions = vec![serde_json::from_value(serde_json::json!({
        "id":"big","name":"Big","kind":"http","http": {
            "url": format!("http://{address}/big"),
            "method": "POST",
            "body": {"text": "{{input}}"},
            "response": {"format":"json"}
        }
    }))
    .unwrap()];
    store.save(snapshot.config, &snapshot.revision).unwrap();
    let huge = "汉字块".repeat(100_000); // 900 KB of UTF-8 text, exceeding the trace body limit
    let id = manager.start("big", huge.clone()).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Succeeded);
    let run = final_snapshot(&manager, &id).await;
    let text = trace_text(&run);
    assert!(!text.contains(&huge), "完整大参数不得进入 trace");
    assert!(text.contains("（超出 trace 容量"), "{text}");
    assert!(run.trace_truncated, "逐条限幅必须反映到 traceTruncated");
    assert!(text.contains("terminal: 完成"), "{text}");
    assert!(trace_bytes(&run.trace) < 300_000, "trace 总量必须有界");
    server.await.unwrap();
}

/// 已读保存失败：返回错误、保留未读；解除占用后显式重试成功并跨重启成立。
#[cfg(windows)]
#[tokio::test]
async fn mark_read_reports_persist_failure_and_retries() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_root, store, manager) = harness();
    let mut config = store.load();
    config.config.actions[0].cwd = Some("Z:/肯定不存在的目录/bullet".into());
    store.save(config.config, &config.revision).unwrap();
    let id = manager.start("normalize-text", "".into()).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Failed);
    assert!(manager.list()[0].unread);

    // 占住 history.json：禁止删除共享，replace 必定失败。
    let history_path = store.root.join("history.json");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&history_path)
        .unwrap();
    let failure = manager.mark_read(&id).unwrap_err();
    assert!(failure.contains("已读确认未能保存"), "{failure}");
    assert!(manager.list()[0].unread, "保存失败必须恢复未读");

    // 显式重试：占用解除后成功，并跨重启成立。
    drop(held);
    manager.mark_read(&id).unwrap();
    assert!(!manager.list()[0].unread);
    drop(manager);
    let reopened = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    let reopened_unread = reopened
        .list()
        .into_iter()
        .find(|run| run.id == id)
        .unwrap()
        .unread;
    assert!(!reopened_unread, "重试成功后重启不得复活未读");
}

/// 旧格式历史（无 trace 字段）加载不报错；订阅不得隐式清除未读。
#[tokio::test]
async fn old_history_loads_and_subscribe_does_not_clear_unread() {
    let (root, store, manager) = harness();
    let old = serde_json::json!([{
        "id":"old-run","actionId":"normalize-text","actionName":"旧任务","input":"旧输入",
        "status":"failed","stage":"失败","stdout":"","stderr":"",
        "cwd":null,"startedAt":1,"finishedAt":2,
        "error":"旧失败","exitCode":1,"httpStatus":null,
        "truncated":false,"submissionUnknown":false,
        "unread":true,"outputMode":"live","configRevision":"old","scriptRevision":null,
        "seq":7
    }]);
    std::fs::write(
        root.path().join("history.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    drop(manager);
    let manager = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    let listed = manager.list();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].unread);
    let run = final_snapshot(&manager, "old-run").await;
    assert!(run.unread, "订阅不得清除未读");
    assert!(run.trace.is_empty() && !run.trace_truncated);
    assert_eq!(run.seq, 7);
}

/// 显式已读跨重启成立；取消不置未读但保留终态 trace。
#[cfg(windows)]
#[tokio::test]
async fn mark_read_persists_and_cancelled_stays_unread_false() {
    let (_root, store, manager) = harness();
    // 长任务：先验证运行期不清未读。
    let mut config = store.load();
    let action = &mut config.config.actions[0];
    action.kind = ActionKind::Cli;
    action.program = "powershell".into();
    action.args = vec![
        "-NoProfile".into(),
        "-Command".into(),
        "Start-Sleep -Seconds 30".into(),
    ];
    store.save(config.config, &config.revision).unwrap();
    let running_id = manager.start("normalize-text", "长任务".into()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!manager.list()[0].unread, "运行中未读必须为假");
    manager.cancel(&running_id).unwrap();
    assert_eq!(wait_for(&manager, &running_id).await, RunStatus::Cancelled);
    let cancelled = final_snapshot(&manager, &running_id).await;
    assert!(!cancelled.unread, "取消不产生提醒");
    assert!(cancelled.trace.iter().any(|entry| entry.step == "terminal"));

    // 失败任务：终态置未读；订阅不清除；显式已读跨重启成立。
    let mut config = store.load();
    config.config.actions[0].cwd = Some("Z:/肯定不存在的目录/bullet".into());
    store.save(config.config, &config.revision).unwrap();
    let failed_id = manager.start("normalize-text", "".into()).unwrap();
    assert_eq!(wait_for(&manager, &failed_id).await, RunStatus::Failed);
    let failed = final_snapshot(&manager, &failed_id).await;
    assert!(failed.unread, "失败终态必须置未读");
    manager.mark_read(&failed_id).unwrap();
    assert!(!manager.list()[0].unread);
    drop(manager);
    let reopened = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    let reopened_unread = reopened
        .list()
        .into_iter()
        .find(|run| run.id == failed_id)
        .unwrap()
        .unread;
    assert!(!reopened_unread, "重启不得复活未读");
    let _ = store;
}
