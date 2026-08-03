use anyhow::Result;
use tokio::process::Command;

pub fn detach_process(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        })
    };

    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
}

pub async fn run_detached(mut command: Command) -> Result<()> {
    detach_process(&mut command);

    let mut child = command.spawn()?;
    let exit_status = child.wait().await?;

    if let Some(code) = exit_status.code()
        && code != 0
    {
        Err(anyhow::anyhow!("Process exited with status code {code}"))
    } else {
        Ok(())
    }
}
