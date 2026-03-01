use anyhow::{Context, Result};
use openssh::{KnownHosts, Session, Stdio};
use std::collections::HashMap;

use crate::config::Server;

pub struct SshPool {
    sessions: HashMap<String, Session>,
}

impl SshPool {
    pub async fn connect(servers: &HashMap<String, Server>) -> Result<Self> {
        let mut sessions = HashMap::new();
        for (name, server) in servers {
            let dest = format!("ssh://{}@{}", server.user, server.host);
            let session = Session::connect(&dest, KnownHosts::Strict)
                .await
                .with_context(|| format!("Failed to connect to {}", name))?;
            sessions.insert(name.clone(), session);
        }
        Ok(Self { sessions })
    }

    pub async fn connect_one(name: &str, server: &Server) -> Result<Self> {
        let dest = format!("ssh://{}@{}", server.user, server.host);
        let session = Session::connect(&dest, KnownHosts::Strict)
            .await
            .with_context(|| format!("Failed to connect to {}", name))?;
        let mut sessions = HashMap::new();
        sessions.insert(name.to_string(), session);
        Ok(Self { sessions })
    }

    pub fn get(&self, server: &str) -> Result<&Session> {
        self.sessions
            .get(server)
            .with_context(|| format!("No connection to server '{}'", server))
    }

    pub async fn exec(&self, server: &str, cmd: &str) -> Result<String> {
        let session = self.get(server)?;
        let output = session
            .command("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .with_context(|| format!("Failed to run command on {}", server))?;

        if !output.status.success() {
            anyhow::bail!(
                "Command failed on {} (exit {}): {}\nstderr: {}",
                server,
                output.status,
                cmd,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub async fn exec_streaming(
        &self,
        server: &str,
        cmd: &str,
    ) -> Result<openssh::Child<&Session>> {
        let session = self.get(server)?;
        let child = session
            .command("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await
            .with_context(|| format!("Failed to run command on {}", server))?;
        Ok(child)
    }

    pub async fn upload_file(
        &self,
        server: &str,
        remote_path: &str,
        content: &str,
    ) -> Result<()> {
        let session = self.get(server)?;
        let escaped = content.replace("'", "'\\''");
        let cmd = format!(
            "cat > {} <<'FLOW_EOF'\n{}\nFLOW_EOF",
            remote_path, escaped
        );
        let output = session
            .command("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .await
            .with_context(|| format!("Failed to upload to {}:{}", server, remote_path))?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to write {}:{}: {}",
                server,
                remote_path,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    pub async fn close(self) -> Result<()> {
        for (_, session) in self.sessions {
            session.close().await?;
        }
        Ok(())
    }
}
