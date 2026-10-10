use super::{output::TextDecoder, ExecutionFailure, ExecutionResult, PreparedAction, RunManager};
use crate::config::model::InputMode;
use std::{process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

pub async fn execute(
    prepared: &PreparedAction,
    manager: &Arc<RunManager>,
    id: &str,
    limit: usize,
) -> Result<ExecutionResult, ExecutionFailure> {
    // Windows names are resolved to .exe/.com/.cmd/.bat before spawning. Pass args through
    // Command so Rust performs its batch-file escaping; never build a raw `cmd /c` string.
    let mut command = Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(prepared.cwd.as_ref().unwrap())
        .envs(&prepared.action.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW: console scripts stay behind the text UI.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command.spawn().map_err(|error| {
        ExecutionFailure::from(format!(
            "无法启动 {}：{error}。请检查程序路径和 PATH",
            prepared.program
        ))
    })?;
    let _tree = ProcessTree::attach(&child).map_err(ExecutionFailure::from)?;
    if let Some(pid) = child.id() {
        manager.trace(
            id,
            "spawn",
            format!("PID {pid}；stdout/stderr 双管道保持排空直到退出"),
        );
    }
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    manager.stage(id, "执行中");

    // Drain both pipes while writing stdin; writing all input first can deadlock a chatty child.
    let write_input = async {
        let result = if prepared.action.input == InputMode::Stdin {
            stdin.write_all(prepared.input.as_bytes()).await
        } else {
            Ok(())
        };
        drop(stdin); // EOF is part of the text-input contract, even for empty input.
        result
    };
    let (input_result, stdout_result, stderr_result, status) = tokio::join!(
        write_input,
        drain(
            stdout,
            &prepared.action.output.encoding,
            manager,
            id,
            false,
            limit
        ),
        drain(
            stderr,
            &prepared.action.output.encoding,
            manager,
            id,
            true,
            limit
        ),
        child.wait(),
    );
    let status = status.map_err(|error| ExecutionFailure::from(error.to_string()))?;
    manager.trace(
        id,
        "stdin",
        match prepared.action.input {
            InputMode::Stdin => match &input_result {
                Ok(()) => format!("stdin 写入 {} 字节", prepared.input.len()),
                Err(error) => format!("stdin 写入失败：{error}"),
            },
            InputMode::Args => "输入经命令行参数传递".into(),
        },
    );
    manager.record_exit(id, status.code());
    stdout_result.map_err(ExecutionFailure::from)?;
    stderr_result.map_err(ExecutionFailure::from)?;
    if !status.success() {
        return Err(format!(
            "进程退出失败，退出码：{}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "信号终止".into())
        )
        .into());
    }
    if let Err(error) = input_result {
        if error.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(format!("输入写入失败：{error}").into());
        }
    }
    Ok(ExecutionResult {
        exit_code: status.code(),
        http_status: None,
    })
}

async fn drain(
    mut pipe: impl AsyncRead + Unpin,
    encoding: &str,
    manager: &RunManager,
    id: &str,
    stderr: bool,
    limit: usize,
) -> Result<(), String> {
    let mut decoder = TextDecoder::new(encoding)?;
    let mut buffer = [0_u8; 8192];
    loop {
        let size = pipe
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        let text = decoder.decode(&buffer[..size], size == 0);
        if !text.is_empty() {
            manager.append(id, stderr, &text, limit);
        }
        if size == 0 {
            break;
        }
    }
    Ok(())
}

#[cfg(unix)]
struct ProcessTree {
    pid: i32,
}
#[cfg(unix)]
impl ProcessTree {
    fn attach(child: &tokio::process::Child) -> Result<Self, String> {
        Ok(Self {
            pid: child.id().ok_or("进程已退出")? as i32,
        })
    }
}
#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        // A process group is private to this run. Stop grandchildren holding inherited output pipes.
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
struct ProcessTree {
    handle: windows_sys::Win32::Foundation::HANDLE,
}
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
unsafe impl Sync for ProcessTree {}
#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &tokio::process::Child) -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error().to_string();
                CloseHandle(job);
                return Err(error);
            }
            let process = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                0,
                child.id().ok_or("进程已退出")?,
            );
            if process.is_null() {
                let error = std::io::Error::last_os_error().to_string();
                CloseHandle(job);
                return Err(error);
            }
            let assigned = AssignProcessToJobObject(job, process);
            let error = std::io::Error::last_os_error().to_string();
            CloseHandle(process);
            if assigned == 0 {
                CloseHandle(job);
                return Err(format!("无法管理进程树：{error}"));
            }
            Ok(Self { handle: job })
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
