use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Player {
    child: Option<Child>,
    current: Option<String>,
    started_at: Option<Instant>,
}

impl Player {
    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        let child = Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "error"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("failed to start ffplay: {err}"))?;
        self.current = Some(path.display().to_string());
        self.started_at = Some(Instant::now());
        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.current = None;
        self.started_at = None;
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or_default()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
