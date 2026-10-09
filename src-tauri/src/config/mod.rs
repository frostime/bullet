pub mod model;
pub mod paths;

use model::{ActionKind, Config};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const DEFAULT_SCRIPT: &str = "import sys\n\ntext = sys.stdin.read()\nprint(text.strip())\n";
const MAX_CONFIG_BYTES: u64 = 2_097_152;
const MAX_SCRIPT_BYTES: usize = 1_048_576;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    pub config: Config,
    pub revision: String,
    pub directory: String,
    pub warning: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct ScriptSnapshot {
    pub source: String,
    pub revision: String,
}

pub struct ConfigStore {
    pub root: PathBuf,
    pub home: PathBuf,
    snapshot: Mutex<ConfigSnapshot>,
    write_lock: Mutex<()>,
}

impl ConfigStore {
    pub fn open(root: PathBuf, home: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join("config.json");
        if !path.exists() {
            atomic_write(&path, include_bytes!("../../defaults.json"))?;
        }
        let bytes = read_bounded(&path, MAX_CONFIG_BYTES)?;
        let config: Config =
            serde_json::from_slice(&bytes).map_err(|error| format!("config.json：{error}"))?;
        config.validate()?;
        let store = Self {
            root: root.clone(),
            home,
            snapshot: Mutex::new(ConfigSnapshot {
                config,
                revision: fingerprint(&bytes),
                directory: root.to_string_lossy().into_owned(),
                warning: None,
            }),
            write_lock: Mutex::new(()),
        };
        let action = store
            .snapshot
            .lock()
            .unwrap()
            .config
            .actions
            .iter()
            .find(|a| a.id == "normalize-text" && a.kind == ActionKind::Script)
            .cloned();
        if let Some(action) = action {
            if !store
                .script_path(&action.id, &action.script_extension)?
                .exists()
            {
                store.save_script(
                    &action.id,
                    &action.script_extension,
                    DEFAULT_SCRIPT,
                    &fingerprint(b""),
                )?;
            }
        }
        Ok(store)
    }

    /// Invalid external edits keep the last valid snapshot; the on-disk revision still blocks stale GUI saves.
    pub fn load(&self) -> ConfigSnapshot {
        let _write = self.write_lock.lock().unwrap();
        let result =
            read_bounded(&self.root.join("config.json"), MAX_CONFIG_BYTES).and_then(|bytes| {
                let config: Config =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                config.validate()?;
                Ok((config, fingerprint(&bytes)))
            });
        let mut snapshot = self.snapshot.lock().unwrap();
        match result {
            Ok((config, revision)) => {
                snapshot.config = config;
                snapshot.revision = revision;
                snapshot.warning = None;
            }
            Err(error) => {
                snapshot.warning = Some(format!("配置文件无效，继续使用上一份有效配置：{error}"))
            }
        }
        snapshot.clone()
    }

    pub fn save(&self, config: Config, expected_revision: &str) -> Result<ConfigSnapshot, String> {
        config.validate()?;
        let _write = self.write_lock.lock().unwrap();
        let path = self.root.join("config.json");
        let existing = read_bounded(&path, MAX_CONFIG_BYTES)?;
        if fingerprint(&existing) != expected_revision {
            return Err("配置已被其他编辑修改，请重新载入后合并".into());
        }
        let bytes = serde_json::to_vec_pretty(&config).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err("配置超过 2 MiB".into());
        }
        atomic_write(&path, &bytes)?;
        let next = ConfigSnapshot {
            config,
            revision: fingerprint(&bytes),
            directory: self.root.to_string_lossy().into_owned(),
            warning: None,
        };
        *self.snapshot.lock().unwrap() = next.clone();
        Ok(next)
    }

    pub fn read_script(&self, id: &str, extension: &str) -> Result<ScriptSnapshot, String> {
        let path = self.script_path(id, extension)?;
        let source = if path.exists() {
            String::from_utf8(read_bounded(&path, MAX_SCRIPT_BYTES as u64)?)
                .map_err(|_| "脚本必须为 UTF-8")?
        } else {
            String::new()
        };
        Ok(ScriptSnapshot {
            revision: fingerprint(source.as_bytes()),
            source,
        })
    }

    pub fn save_script(
        &self,
        id: &str,
        extension: &str,
        source: &str,
        expected_revision: &str,
    ) -> Result<ScriptSnapshot, String> {
        if source.len() > MAX_SCRIPT_BYTES {
            return Err("脚本超过 1 MiB".into());
        }
        let _write = self.write_lock.lock().unwrap();
        let existing = self.read_script(id, extension)?;
        if existing.revision != expected_revision {
            return Err("脚本已修改，请重新载入后保存".into());
        }
        let path = self.script_path(id, extension)?;
        atomic_write(&path, source.as_bytes())?;
        Ok(ScriptSnapshot {
            source: source.into(),
            revision: fingerprint(source.as_bytes()),
        })
    }

    pub fn freeze_script(&self, id: &str, extension: &str) -> Result<PathBuf, String> {
        let source = self.read_script(id, extension)?;
        if source.source.trim().is_empty() {
            return Err("脚本为空，请先在配置页编辑并保存".into());
        }
        let version_path = self
            .root
            .join("scripts")
            .join(id)
            .join("versions")
            .join(&source.revision)
            .join(format!("main.{extension}"));
        let _write = self.write_lock.lock().unwrap();
        if !version_path.exists() {
            atomic_write(&version_path, source.source.as_bytes())?;
        }
        Ok(version_path)
    }

    fn script_path(&self, id: &str, extension: &str) -> Result<PathBuf, String> {
        model::validate_id(id)?;
        if extension.is_empty()
            || extension.len() > 10
            || !extension.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            return Err("无效的脚本扩展名".into());
        }
        Ok(self
            .root
            .join("scripts")
            .join(id)
            .join(format!("main.{extension}")))
    }
}

pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("文件超过容量限制：{}", path.display()));
    }
    Ok(bytes)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let directory = path.parent().ok_or("缺少父目录")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub bubble_x: Option<i32>,
    pub bubble_y: Option<i32>,
    pub last_action: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_edit_conflict_and_script_version_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path().to_path_buf(), dir.path().to_path_buf()).unwrap();
        let before = store.load();
        fs::write(dir.path().join("config.json"), b"broken").unwrap();
        assert!(store.load().warning.is_some());
        assert!(store.save(before.config, &before.revision).is_err());
        let frozen = store.freeze_script("normalize-text", "py").unwrap();
        let old = fs::read_to_string(&frozen).unwrap();
        let script = store.read_script("normalize-text", "py").unwrap();
        store
            .save_script("normalize-text", "py", "print('new')", &script.revision)
            .unwrap();
        assert_eq!(fs::read_to_string(frozen).unwrap(), old);
        assert!(store.read_script("../escape", "py").is_err());
    }
}
