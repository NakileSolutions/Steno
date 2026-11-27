//! Lancement d'un CLI d'agent : entrée standard, lecture ligne à ligne, délai et annulation.

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub struct ProcOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr_tail: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

impl ProcOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out && !self.cancelled
    }

    /// Message d'erreur lisible.
    pub fn error_message(&self, what: &str) -> String {
        if self.cancelled {
            return "annulée".into();
        }
        if self.timed_out {
            return format!("{what} : délai dépassé");
        }
        let tail = self.stderr_tail.trim();
        let tail = if tail.is_empty() { self.stdout.trim() } else { tail };
        let tail: String = tail.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{what} a échoué (code {:?}) : {tail}", self.code)
    }
}

#[cfg(unix)]
fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: Option<u32>) {}

/// Lance la commande, écrit `stdin_data`, appelle `on_line` pour chaque ligne de stdout.
pub async fn run_streaming(
    mut cmd: Command,
    stdin_data: Option<String>,
    timeout: Duration,
    cancel: CancellationToken,
    mut on_line: impl FnMut(&str) + Send,
) -> Result<ProcOutput> {
    cmd.stdin(if stdin_data.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().context("impossible de lancer l'agent")?;
    let pid = child.id();
    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                let _ = stdin.write_all(data.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }
    }
    let stdout = child.stdout.take().context("stdout")?;
    let mut stderr = child.stderr.take().context("stderr")?;
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        let s = String::from_utf8_lossy(&buf).to_string();
        let n = s.chars().count();
        s.chars().skip(n.saturating_sub(4000)).collect::<String>()
    });
    let mut out = ProcOutput::default();
    let mut lines = BufReader::new(stdout).lines();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::select! {
            l = lines.next_line() => match l {
                Ok(Some(line)) => {
                    on_line(&line);
                    out.stdout.push_str(&line);
                    out.stdout.push('\n');
                }
                _ => break,
            },
            _ = tokio::time::sleep_until(deadline) => {
                out.timed_out = true;
                kill_group(pid);
                let _ = child.kill().await;
                break;
            }
            _ = cancel.cancelled() => {
                out.cancelled = true;
                kill_group(pid);
                let _ = child.kill().await;
                break;
            }
        }
    }
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    if let Ok(Ok(s)) = status {
        out.code = s.code();
    } else {
        kill_group(pid);
        let _ = child.kill().await;
    }
    out.stderr_tail = tokio::time::timeout(Duration::from_secs(2), err_task)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sortie_et_delai() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("cat; echo fin");
        let mut seen = vec![];
        let o = run_streaming(cmd, Some("bonjour\n".into()), Duration::from_secs(5), CancellationToken::new(), |l| {
            seen.push(l.to_string())
        })
        .await
        .unwrap();
        assert!(o.success());
        assert_eq!(seen, vec!["bonjour", "fin"]);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 10");
        let o = run_streaming(cmd, None, Duration::from_millis(200), CancellationToken::new(), |_| {}).await.unwrap();
        assert!(o.timed_out);

        let tok = CancellationToken::new();
        let t2 = tok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            t2.cancel();
        });
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 10");
        let o = run_streaming(cmd, None, Duration::from_secs(10), tok, |_| {}).await.unwrap();
        assert!(o.cancelled);
    }
}
