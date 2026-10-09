#![cfg(windows)]

use bullet_core::{
    config::{
        model::{ActionKind, InputMode},
        ConfigStore,
    },
    execution::{RunEvent, RunManager, RunSnapshot, RunStatus},
};
use std::sync::{Arc, Mutex};

fn harness(extension: &str) -> (tempfile::TempDir, Arc<ConfigStore>, Arc<RunManager>) {
    let root = tempfile::Builder::new()
        .prefix("bullet CLI test ")
        .tempdir()
        .unwrap();
    let bin = root.path().join("configured bin");
    std::fs::create_dir(&bin).unwrap();
    let shim = concat!(
        "@echo off\r\n",
        "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" ",
        "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"%~dp0echo.ps1\" %*\r\n",
    );
    std::fs::write(bin.join(format!("bullet-test-cli.{extension}")), shim).unwrap();
    std::fs::write(bin.join("echo.ps1"), concat!(
        "$utf8 = New-Object System.Text.UTF8Encoding($false)\r\n",
        "[Console]::InputEncoding = $utf8\r\n[Console]::OutputEncoding = $utf8\r\n",
        "if ($args.Count -gt 0) { [Console]::Out.Write((ConvertTo-Json -InputObject @($args) -Compress)) }\r\n",
        "else { [Console]::Out.Write([Console]::In.ReadToEnd()) }\r\n",
        "Start-Sleep -Milliseconds 250\r\n",
    )).unwrap();
    let store =
        Arc::new(ConfigStore::open(root.path().to_owned(), root.path().to_owned()).unwrap());
    let mut config = store.load();
    let action = &mut config.config.actions[0];
    action.kind = ActionKind::Cli;
    action.program = "bullet-test-cli".into();
    action.args.clear();
    action
        .env
        .insert("Path".into(), bin.to_string_lossy().into_owned());
    action.env.insert("PATHEXT".into(), ".EXE;.CMD;.BAT".into());
    store.save(config.config, &config.revision).unwrap();
    let runs = RunManager::new(store.clone(), Arc::new(|_, _| {}));
    (root, store, runs)
}

async fn completed(runs: &RunManager, id: &str) -> RunSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if runs
                .list()
                .into_iter()
                .find(|run| run.id == id)
                .unwrap()
                .status
                != RunStatus::Running
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = Arc::new(Mutex::new(None));
    let received = snapshot.clone();
    runs.subscribe(
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

#[tokio::test]
async fn cmd_name_uses_action_path_and_preserves_multiline_stdin() {
    let (_root, _store, runs) = harness("cmd");
    let input = "第一行\n第二行 \"quotes\" & symbols";
    let id = runs.start("normalize-text", input.into()).unwrap();
    let run = completed(&runs, &id).await;
    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{:?} {}",
        run.error,
        run.stderr
    );
    assert_eq!(run.stdout, input);
}

#[tokio::test]
async fn bat_name_keeps_configured_arguments_separate() {
    let (_root, store, runs) = harness("bat");
    let mut config = store.load();
    let action = &mut config.config.actions[0];
    action.input = InputMode::Args;
    action.args = vec!["before".into(), "{{input}}".into(), "after".into()];
    store.save(config.config, &config.revision).unwrap();
    let input = "text with spaces & symbols";
    let id = runs.start("normalize-text", input.into()).unwrap();
    let run = completed(&runs, &id).await;
    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{:?} {}",
        run.error,
        run.stderr
    );
    let args: Vec<String> = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(args, vec!["before", input, "after"]);
}

#[test]
fn explicit_cmd_path_and_relative_path_entries_use_configured_cwd() {
    use bullet_core::config::paths::resolve_program;
    let (root, store, _runs) = harness("cmd");
    let home = root.path();
    let action = store.load().config.actions.remove(0);
    let found =
        resolve_program("./configured bin/bullet-test-cli", home, home, &action.env).unwrap();
    assert_eq!(
        std::path::PathBuf::from(found),
        home.join("configured bin/bullet-test-cli.cmd")
    );
    let mut env = action.env;
    env.insert("Path".into(), "configured bin".into());
    assert!(resolve_program("bullet-test-cli", home, home, &env)
        .unwrap()
        .ends_with(".cmd"));
    env.insert("Path".into(), "".into());
    assert!(resolve_program("bullet-test-cli", home, home, &env).is_err());
}
