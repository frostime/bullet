use std::path::{Path, PathBuf};

pub fn expand_home(path: &str, home: &Path) -> Result<PathBuf, String> {
    if path == "~" {
        return Ok(home.to_path_buf());
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return Ok(home.join(rest));
    }
    if path.starts_with('~') {
        return Err("仅支持 ~ 或 ~/...，不支持 ~用户名".into());
    }
    Ok(PathBuf::from(path))
}

pub fn resolve_cwd(default: &str, action: Option<&str>, home: &Path) -> Result<PathBuf, String> {
    let base = expand_home(default, home)?;
    if !base.is_absolute() {
        return Err("默认工作目录必须是绝对路径".into());
    }
    let cwd = match action.filter(|value| !value.trim().is_empty()) {
        None => base,
        Some(value) => {
            let candidate = expand_home(value, home)?;
            if candidate.is_absolute() {
                candidate
            } else {
                base.join(candidate)
            }
        }
    };
    if !cwd.is_dir() {
        return Err(format!("工作目录不存在：{}", cwd.display()));
    }
    cwd.canonicalize().map_err(|error| error.to_string())
}

pub fn resolve_program(program: &str, cwd: &Path, home: &Path) -> Result<String, String> {
    let path = expand_home(program, home)?;
    if path.is_absolute() || program.contains('/') || program.contains('\\') {
        let absolute = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        return Ok(absolute.to_string_lossy().into_owned());
    }
    Ok(program.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_action_cwd_uses_default_not_launcher() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("work")).unwrap();
        assert_eq!(
            resolve_cwd("~", Some("work"), root.path()).unwrap(),
            root.path().join("work").canonicalize().unwrap()
        );
        assert!(resolve_cwd("~", Some("missing"), root.path()).is_err());
        assert!(expand_home("~someone", root.path()).is_err());
    }
}
