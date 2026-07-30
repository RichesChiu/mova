use std::{io, process::ExitStatus, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

#[derive(Debug)]
pub struct BoundedCommandOutput {
    pub status: ExitStatus,
    pub stderr: Vec<u8>,
    pub stderr_truncated: bool,
}

#[derive(Debug)]
pub enum BoundedCommandError {
    Spawn(io::Error),
    Io(io::Error),
    TimedOut,
}

pub async fn run_with_bounded_stderr(
    command: &mut Command,
    deadline: Duration,
    stderr_limit: usize,
) -> Result<BoundedCommandOutput, BoundedCommandError> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(BoundedCommandError::Spawn)?;
    let stderr = child.stderr.take().ok_or_else(|| {
        BoundedCommandError::Io(io::Error::other("child process stderr was not piped"))
    })?;

    let result = timeout(deadline, async {
        let (status, (stderr, stderr_truncated)) =
            tokio::try_join!(child.wait(), read_bounded(stderr, stderr_limit))?;
        Ok::<_, io::Error>(BoundedCommandOutput {
            status,
            stderr,
            stderr_truncated,
        })
    })
    .await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(BoundedCommandError::Io(error))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(BoundedCommandError::TimedOut)
        }
    }
}

async fn read_bounded(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }

        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }

    Ok((retained, truncated))
}

#[cfg(test)]
mod tests {
    use super::read_bounded;

    #[tokio::test]
    async fn bounded_reader_drains_input_without_retaining_the_overflow() {
        let input = std::io::Cursor::new(vec![b'x'; 1024]);
        let (retained, truncated) = read_bounded(input, 64).await.unwrap();

        assert_eq!(retained.len(), 64);
        assert!(truncated);
    }
}
