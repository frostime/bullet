use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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

/// Syntax validation does not require the directory to exist.
pub fn validate_cwd(value: &str, allow_relative: bool) -> Result<(), String> {
    let value = value.trim();
    if value.starts_with('~') {
        if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
            return Ok(());
        }
        return Err("仅支持 ~、~/... 或 ~\\...，不支持 ~用户名".into());
    }
    let path = Path::new(value);
    if value.is_empty() || (!allow_relative && !path.is_absolute()) {
        return Err("默认工作目录必须是绝对路径或 ~/...".into());
    }
    #[cfg(windows)]
    if path.has_root() != path.is_absolute()
        || (matches!(
            path.components().next(),
            Some(std::path::Component::Prefix(_))
        ) && !path.is_absolute())
    {
        return Err("工作目录不能使用驱动器相对路径或无驱动器的根路径".into());
    }
    Ok(())
}

pub fn resolve_cwd(default: &str, action: Option<&str>, home: &Path) -> Result<PathBuf, String> {
    validate_cwd(default, false)?;
    let base = expand_home(default.trim(), home)?;
    if !base.is_absolute() {
        return Err("默认工作目录必须是绝对路径".into());
    }
    let cwd = match action.filter(|value| !value.trim().is_empty()) {
        None => base,
        Some(value) => {
            validate_cwd(value, true)?;
            let candidate = expand_home(value.trim(), home)?;
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

pub fn resolve_program(
    program: &str,
    cwd: &Path,
    home: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<String, String> {
    let path = expand_home(program, home)?;
    let has_path = path.is_absolute()
        || path.components().count() > 1
        || program.contains('/')
        || program.contains('\\');
    #[cfg(windows)]
    {
        let search_path = windows_environment(environment, "PATH").unwrap_or_default();
        let extensions = windows_extensions(windows_environment(environment, "PATHEXT").as_deref());
        let candidates = if has_path {
            let absolute = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };
            if !absolute.is_absolute() {
                return Err(
                    "程序路径不能使用 C:foo 这样的驱动器相对路径，请使用绝对路径或 ./...".into(),
                );
            }
            vec![absolute]
        } else {
            std::env::split_paths(&search_path)
                .filter(|directory| !directory.as_os_str().is_empty())
                .filter_map(|directory| {
                    let directory = if directory.is_absolute() {
                        directory
                    } else {
                        cwd.join(directory)
                    };
                    directory.is_absolute().then(|| directory.join(&path))
                })
                .collect()
        };
        return find_windows_program(&candidates, &extensions)
            .map(|found| found.to_string_lossy().into_owned())
            .ok_or_else(|| format!("找不到程序 {program}：请检查 bullet 的 PATH 或动作环境变量中的 PATH。Windows 支持 .exe/.com/.cmd/.bat；修改系统 PATH 后请重启 bullet。"));
    }
    #[cfg(not(windows))]
    {
        let _ = environment; // Command uses the child's PATH on Unix.
        if has_path {
            let absolute = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };
            Ok(absolute.to_string_lossy().into_owned())
        } else {
            Ok(program.to_string())
        }
    }
}

#[cfg(windows)]
fn windows_environment(
    environment: &BTreeMap<String, String>,
    name: &str,
) -> Option<std::ffi::OsString> {
    // Match Command::envs: Windows keys ignore case and the last configured spelling wins.
    environment
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.into())
        .or_else(|| std::env::var_os(name))
}

#[cfg(any(windows, test))]
fn windows_extensions(value: Option<&std::ffi::OsStr>) -> Vec<String> {
    let value = value
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
    let mut extensions = Vec::new();
    for extension in value
        .split(';')
        .map(|part| part.trim().to_ascii_lowercase())
    {
        if [".com", ".exe", ".bat", ".cmd"].contains(&extension.as_str())
            && !extensions.contains(&extension)
        {
            extensions.push(extension);
        }
    }
    extensions
}

#[cfg(any(windows, test))]
fn find_windows_program(candidates: &[PathBuf], extensions: &[String]) -> Option<PathBuf> {
    for candidate in candidates {
        if candidate.extension().is_some() && candidate.is_file() {
            return Some(candidate.clone());
        }
        // npm also creates an extensionless Unix shim. Prefer the Windows entry point.
        for extension in extensions {
            let mut filename = candidate.as_os_str().to_os_string();
            filename.push(extension);
            let file = PathBuf::from(filename);
            if file.is_file() {
                return Some(file);
            }
        }
    }
    None
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

    #[test]
    fn windows_name_finds_npm_cmd_instead_of_extensionless_shim() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("pi"), "#!/bin/sh\n").unwrap();
        std::fs::write(root.path().join("pi.ps1"), "# PowerShell shim").unwrap();
        let cmd = root.path().join("pi.cmd");
        std::fs::write(&cmd, "@echo off\r\n").unwrap();
        assert_eq!(
            find_windows_program(&[root.path().join("pi")], &windows_extensions(None)),
            Some(cmd)
        );
    }

    #[test]
    fn windows_search_preserves_path_order_and_extension_preference() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("tool.cmd"), "").unwrap();
        std::fs::write(second.join("tool.exe"), "").unwrap();
        let candidates = [first.join("tool"), second.join("tool")];
        let preferred = windows_extensions(Some(std::ffi::OsStr::new(".EXE;.CMD;.exe;.PS1")));
        assert_eq!(preferred, vec![".exe", ".cmd"]);
        assert_eq!(
            find_windows_program(&candidates, &preferred),
            Some(first.join("tool.cmd"))
        );
        std::fs::write(first.join("tool.exe"), "").unwrap();
        assert_eq!(
            find_windows_program(&candidates, &preferred),
            Some(first.join("tool.exe"))
        );
        let cmd_first = windows_extensions(Some(std::ffi::OsStr::new(".CMD;.EXE")));
        assert_eq!(
            find_windows_program(&candidates, &cmd_first),
            Some(first.join("tool.cmd"))
        );
    }

    #[test]
    fn windows_explicit_extension_is_not_replaced_and_directories_are_not_programs() {
        let root = tempfile::tempdir().unwrap();
        let explicit = root.path().join("pi.cmd");
        std::fs::write(&explicit, "").unwrap();
        std::fs::write(root.path().join("pi.exe"), "").unwrap();
        std::fs::create_dir(root.path().join("missing.cmd")).unwrap();
        let extensions = windows_extensions(None);
        assert_eq!(
            find_windows_program(&[explicit.clone()], &extensions),
            Some(explicit)
        );
        assert_eq!(
            find_windows_program(&[root.path().join("missing")], &extensions),
            None
        );
        assert_eq!(find_windows_program(&[root.path().join("pi")], &[]), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_bare_command_remains_a_path_lookup() {
        let root = tempfile::tempdir().unwrap();
        let env = BTreeMap::new();
        assert_eq!(
            resolve_program("pi", root.path(), root.path(), &env).unwrap(),
            "pi"
        );
        assert_eq!(
            resolve_program("./bin/pi", root.path(), root.path(), &env).unwrap(),
            root.path().join("./bin/pi").to_string_lossy()
        );
    }
}
