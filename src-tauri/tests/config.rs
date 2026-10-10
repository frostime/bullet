use bullet_core::config::{
    atomic_write, fingerprint,
    model::{ActionKind, Config},
    paths::resolve_cwd,
    read_bounded, ConfigSnapshot, ConfigStore, DiskStatus,
};
use std::{fs, path::Path};

fn fixture() -> (tempfile::TempDir, ConfigStore, ConfigSnapshot) {
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigStore::open(dir.path().to_path_buf(), dir.path().to_path_buf()).unwrap();
    let snapshot = store.load();
    (dir, store, snapshot)
}

fn rejected<T>(result: Result<T, String>) -> String {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn invalid_disk_versions_keep_last_valid_revision_but_have_distinct_disk_revisions() {
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    let mut invalid_config = before.config.clone();
    invalid_config.version = 2;
    let invalid_schema = serde_json::to_vec(&invalid_config).unwrap();
    for bytes in [b"not json".as_slice(), invalid_schema.as_slice()] {
        fs::write(&path, bytes).unwrap();
        let snapshot = store.load();
        assert_eq!(snapshot.disk_status, DiskStatus::Invalid);
        assert_eq!(snapshot.disk_revision, Some(fingerprint(bytes)));
        assert_eq!(snapshot.revision, before.revision);
        assert_eq!(snapshot.config.version, 1);
        assert!(snapshot.warning.unwrap().contains("上一份有效配置"));
        // Even the exact invalid disk hash must not authorize an ordinary save.
        assert!(
            rejected(store.save(before.config.clone(), &fingerprint(bytes))).contains("明确恢复")
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(ConfigStore::open(dir.path().to_path_buf(), dir.path().to_path_buf()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unreadable_and_oversized_disk_have_no_recovery_revision() {
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    fs::rename(&path, dir.path().join("original.json")).unwrap();
    for oversized in [false, true] {
        if oversized {
            fs::write(&path, vec![b' '; 2_097_153]).unwrap();
        }
        let snapshot = store.load();
        assert_eq!(snapshot.disk_status, DiskStatus::Unavailable);
        assert_eq!(snapshot.disk_revision, None);
        assert_eq!(snapshot.revision, before.revision);
        assert!(snapshot.warning.is_some());
        assert!(store
            .restore(before.config.clone(), &before.revision)
            .is_err());
    }
    let missing = dir.path().join("missing.json");
    let error = rejected(read_bounded(&missing, 100));
    assert!(error.contains(&missing.display().to_string()));
    assert!(error.contains("open"));
    assert!(error.contains("os code: Some("));
}

#[test]
fn explicit_restore_preserves_rejected_bytes_and_reports_backup() {
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    let invalid = b"{ rejected source\n";
    fs::write(&path, invalid).unwrap();
    let disk = store.load().disk_revision.unwrap();
    let restored = store.restore(before.config, &disk).unwrap();
    let backup = dir.path().join("recovery").join(format!("{disk}.json"));
    assert_eq!(fs::read(&backup).unwrap(), invalid);
    assert!(restored
        .warning
        .unwrap()
        .contains(&backup.display().to_string()));
    assert_eq!(restored.disk_status, DiskStatus::Valid);
    assert_eq!(
        restored.disk_revision.as_deref(),
        Some(restored.revision.as_str())
    );
    assert_eq!(store.load().revision, restored.revision);
    let saved = store.save(restored.config, &restored.revision).unwrap();
    assert_eq!(saved.disk_status, DiskStatus::Valid);
    assert!(saved.warning.is_none());
}

#[test]
fn stale_restore_and_restore_of_valid_disk_never_replace_content() {
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    fs::write(&path, b"invalid A").unwrap();
    let stale = store.load().disk_revision.unwrap();
    fs::write(&path, b"invalid B").unwrap();
    assert!(rejected(store.restore(before.config.clone(), &stale)).contains("已变化"));
    assert_eq!(fs::read(&path).unwrap(), b"invalid B");
    assert!(!dir.path().join("recovery").exists());
    let valid = serde_json::to_vec(&before.config).unwrap();
    fs::write(&path, &valid).unwrap();
    assert!(
        rejected(store.restore(before.config.clone(), &fingerprint(&valid))).contains("正常保存")
    );
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(store.save(before.config, &before.revision).is_err());
    assert_eq!(fs::read(&path).unwrap(), valid);
}

#[test]
fn backup_creation_or_verification_failure_leaves_invalid_source_untouched() {
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    let invalid = b"invalid source";
    fs::write(&path, invalid).unwrap();
    let disk = fingerprint(invalid);
    let recovery = dir.path().join("recovery");
    fs::write(&recovery, b"not a directory").unwrap();
    assert!(store.restore(before.config.clone(), &disk).is_err());
    assert_eq!(fs::read(&path).unwrap(), invalid);
    fs::remove_file(&recovery).unwrap();
    fs::create_dir(&recovery).unwrap();
    let backup = recovery.join(format!("{disk}.json"));
    fs::write(&backup, b"corrupted backup").unwrap();
    assert!(rejected(store.restore(before.config, &disk)).contains("备份验证失败"));
    assert_eq!(fs::read(&path).unwrap(), invalid);
    assert_eq!(fs::read(backup).unwrap(), b"corrupted backup");
}

#[test]
fn disabled_actions_skip_execution_completeness_but_not_identity_or_path_safety() {
    let (_dir, _store, before) = fixture();
    let mut config = before.config;
    config.actions.truncate(1);
    let action = &mut config.actions[0];
    action.kind = ActionKind::Script;
    action.enabled = false;
    action.args.clear();
    action.program.clear();
    action.timeout_sec = Some(0);
    action.output.encoding = "unknown encoding".into();
    config.validate().unwrap();
    for (id, extension, cwd) in [
        ("../escape", "py", None),
        ("safe", "../py", None),
        ("safe", "py", Some("~someone/work")),
    ] {
        let mut invalid = config.clone();
        invalid.actions[0].id = id.into();
        invalid.actions[0].script_extension = extension.into();
        invalid.actions[0].cwd = cwd.map(String::from);
        let error = invalid.validate().unwrap_err();
        assert!(error.contains(&invalid.actions[0].id));
        assert!(error.contains(&invalid.actions[0].name));
    }
    config.actions[0].enabled = true;
    config.actions[0].timeout_sec = None;
    config.actions[0].output.encoding = "utf-8".into();
    config.actions[0].program = "python".into();
    assert!(config.validate().unwrap_err().contains("{{script}}"));
    config.actions[0].args = vec!["{{script}}".into()];
    config.validate().unwrap();
    config.actions[0].kind = ActionKind::Http;
    config.actions[0].http = None;
    assert!(config
        .validate()
        .unwrap_err()
        .contains(&config.actions[0].id));
    config.actions[0].enabled = false;
    config.validate().unwrap();
}

#[test]
fn cwd_validation_and_execution_share_trimmed_home_syntax() {
    let (dir, store, before) = fixture();
    fs::create_dir(dir.path().join("work")).unwrap();
    for cwd in [" ~ ", " ~/ ", " ~\\ "] {
        let mut config = before.config.clone();
        config.settings.default_cwd = cwd.into();
        config.validate().unwrap();
        assert_eq!(
            resolve_cwd(cwd, None, dir.path()).unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }
    for cwd in [" ~/work ", " ~\\work ", " work "] {
        let mut config = before.config.clone();
        config.actions[0].cwd = Some(cwd.into());
        config.validate().unwrap();
        assert_eq!(
            resolve_cwd(" ~ ", Some(cwd), dir.path()).unwrap(),
            dir.path().join("work").canonicalize().unwrap()
        );
    }
    let mut config = before.config.clone();
    config.settings.default_cwd = " ~person ".into();
    assert!(config.validate().is_err());
    assert!(resolve_cwd(" ~person ", None, dir.path()).is_err());
    config.settings.default_cwd = " ~ ".into();
    config.actions[0].cwd = Some(" work ".into());
    config.actions[0].program = " python ".into();
    config.actions[0].args.push(" untouched argument ".into());
    let saved = store.save(config, &before.revision).unwrap();
    assert_eq!(saved.config.settings.default_cwd, "~");
    assert_eq!(saved.config.actions[0].cwd.as_deref(), Some("work"));
    assert_eq!(saved.config.actions[0].program, " python ");
    assert_eq!(
        saved.config.actions[0].args.last().unwrap(),
        " untouched argument "
    );
    // External loads normalize exactly the same fields without rewriting the source.
    let mut external = saved.config;
    external.settings.default_cwd = " ~ ".into();
    external.actions[0].cwd = Some(" work ".into());
    let bytes = serde_json::to_vec(&external).unwrap();
    fs::write(dir.path().join("config.json"), &bytes).unwrap();
    let loaded = store.load();
    assert_eq!(loaded.config.settings.default_cwd, "~");
    assert_eq!(loaded.config.actions[0].cwd.as_deref(), Some("work"));
    assert_eq!(loaded.revision, fingerprint(&bytes));
    assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), bytes);
}

fn assert_replace_error(error: &str, path: &Path) {
    assert!(error.contains(&path.display().to_string()), "{error}");
    assert!(error.contains("replace"), "{error}");
    assert!(error.contains("os code: Some("), "{error}");
}

#[cfg(windows)]
#[test]
fn windows_readonly_replace_reports_target_and_preserves_content_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path, b"old").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let result = atomic_write(&path, b"new");
    let readonly = fs::metadata(&path).unwrap().permissions().readonly();
    let content = fs::read(&path).unwrap();
    // Test cleanup only; the application must never clear this attribute.
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    assert_replace_error(&rejected(result), &path);
    assert!(readonly);
    assert_eq!(content, b"old");
}

#[cfg(windows)]
#[test]
fn windows_no_delete_sharing_blocks_restore_but_explicit_retry_reuses_backup() {
    use std::os::windows::fs::OpenOptionsExt;
    let (dir, store, before) = fixture();
    let path = dir.path().join("config.json");
    let invalid = b"invalid locked source";
    fs::write(&path, invalid).unwrap();
    let disk = fingerprint(invalid);
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&path)
        .unwrap();
    let error = rejected(store.restore(before.config.clone(), &disk));
    assert_replace_error(&error, &path);
    assert_eq!(fs::read(&path).unwrap(), invalid);
    let backup = dir.path().join("recovery").join(format!("{disk}.json"));
    assert_eq!(fs::read(&backup).unwrap(), invalid);
    assert!(error.contains(&backup.display().to_string()));
    drop(held);
    let restored = store.restore(before.config, &disk).unwrap();
    assert_eq!(restored.disk_status, DiskStatus::Valid);
    assert_eq!(fs::read(backup).unwrap(), invalid);
}

#[test]
fn serde_draft_defaults_do_not_require_execution_fields_and_reject_unknown_fields() {
    let draft =
        r#"{"version":1,"settings":{},"actions":[{"id":"draft","name":"Draft","kind":"script"}]}"#;
    let config: Config = serde_json::from_str(draft).unwrap();
    assert_eq!(config.settings.default_cwd, "~");
    assert_eq!(config.actions[0].script_extension, "py");
    assert!(config.validate().is_err());
    assert!(serde_json::from_str::<Config>(
        &draft.replace("\"settings\":{}", "\"settings\":{\"typo\":true}")
    )
    .is_err());
}
