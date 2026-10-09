use bullet_core::{
    config::{
        model::{ActionKind, NotificationMode},
        ConfigStore,
    },
    execution::{RunEvent, RunManager, RunStatus},
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
async fn wait_for(manager: &RunManager, id: &str) -> RunStatus {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let state = manager
                .list()
                .into_iter()
                .find(|run| run.id == id)
                .unwrap()
                .status;
            if state != RunStatus::Running {
                break state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
#[cfg(unix)]
#[tokio::test]
async fn concurrent_processes_receive_eof_and_detach_does_not_cancel() {
    let (_root, store, manager) = harness();
    let mut config = store.load();
    config.config.settings.notifications = NotificationMode::Off;
    let action = &mut config.config.actions[0];
    action.kind = ActionKind::Cli;
    action.program = "python3".into();
    action.args = vec!["-u".into(),"-c".into(),"import sys,time,os; s=sys.stdin.read(); print(os.getcwd(),flush=True); time.sleep(.15); print(s,flush=True)".into()];
    store.save(config.config, &config.revision).unwrap();
    let first = manager.start("normalize-text", "甲\n乙".into()).unwrap();
    let second = manager.start("normalize-text", "丙".into()).unwrap();
    manager.detach(&first);
    assert_eq!(wait_for(&manager, &first).await, RunStatus::Succeeded);
    assert_eq!(wait_for(&manager, &second).await, RunStatus::Succeeded);
    let snapshot = Arc::new(Mutex::new(None));
    let received = snapshot.clone();
    manager
        .subscribe(
            &first,
            Arc::new(move |event| {
                if let RunEvent::Snapshot { run } = event {
                    *received.lock().unwrap() = Some(run);
                }
                true
            }),
        )
        .unwrap();
    let run = snapshot.lock().unwrap().clone().unwrap();
    assert!(run.stdout.contains("甲\n乙"));
    assert_eq!(run.exit_code, Some(0));
    assert!(run.stdout.contains(run.cwd.as_ref().unwrap()));
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.root.join("history.json")).unwrap()).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 2);
}
#[cfg(unix)]
#[tokio::test]
async fn cancellation_retains_partial_output() {
    let (_root, store, manager) = harness();
    let mut config = store.load();
    let action = &mut config.config.actions[0];
    action.kind = ActionKind::Cli;
    action.program = "python3".into();
    action.args = vec![
        "-u".into(),
        "-c".into(),
        "import time; print('partial',flush=True); time.sleep(30)".into(),
    ];
    store.save(config.config, &config.revision).unwrap();
    let id = manager.start("normalize-text", "".into()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(160)).await;
    manager.cancel(&id).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Cancelled);
    let output = Arc::new(Mutex::new(String::new()));
    let received = output.clone();
    manager
        .subscribe(
            &id,
            Arc::new(move |event| {
                if let RunEvent::Snapshot { run } = event {
                    *received.lock().unwrap() = run.stdout;
                }
                true
            }),
        )
        .unwrap();
    assert!(output.lock().unwrap().contains("partial"));
}
#[tokio::test]
async fn http_sse_extracts_deltas_across_transport_chunks() {
    let (_root, store, manager) = harness();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 8192];
        let _ = socket.read(&mut request).await.unwrap();
        let body =
            "data: {\"delta\":\"你好\"}\r\n\r\ndata: {\"delta\":\"世界\"}\n\ndata: [DONE]\n\n";
        let header=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
        socket.write_all(header.as_bytes()).await.unwrap();
        for chunk in body.as_bytes().chunks(7) {
            socket.write_all(chunk).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    });
    let mut snapshot = store.load();
    snapshot.config.actions=vec![serde_json::from_value(serde_json::json!({
        "id":"stream","name":"Stream","kind":"http","http": {
            "url":format!("http://{address}"),"method":"GET","response":{"format":"sse","pointer":"/delta","doneMarker":"[DONE]"}
        }
    })).unwrap()];
    store.save(snapshot.config, &snapshot.revision).unwrap();
    let id = manager.start("stream", "hello".into()).unwrap();
    assert_eq!(wait_for(&manager, &id).await, RunStatus::Succeeded);
    let output = Arc::new(Mutex::new(String::new()));
    let received = output.clone();
    manager
        .subscribe(
            &id,
            Arc::new(move |event| {
                if let RunEvent::Snapshot { run } = event {
                    *received.lock().unwrap() = run.stdout;
                }
                true
            }),
        )
        .unwrap();
    assert_eq!(*output.lock().unwrap(), "你好世界");
    server.await.unwrap();
}
