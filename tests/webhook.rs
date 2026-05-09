#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use iron::webhook;

static SERVER_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn compose_includes_secret_and_image() {
    let yaml = webhook::generate_compose("flow", "shhh", "fl-1", None, None, None);
    assert!(yaml.contains(webhook::IMAGE));
    assert!(yaml.contains("GITHUB_WEBHOOK_SECRET: shhh"));
    assert!(yaml.contains("BIND: 0.0.0.0:8080"));
    assert!(yaml.contains("IRON_SERVER: fl-1"));
    assert!(yaml.contains("./webhook.py:/webhook.py:ro"));
    assert!(yaml.contains("- flow"));
    assert!(yaml.contains("external: true"));
}

#[test]
fn compose_includes_tail_env_when_set() {
    let yaml = webhook::generate_compose(
        "flow",
        "s",
        "fl-1",
        Some("https://tail.example.com"),
        Some("test@example.com"),
        Some("hunter2"),
    );
    assert!(yaml.contains("TAIL_URL: https://tail.example.com"));
    assert!(yaml.contains("TAIL_USER: test@example.com"));
    assert!(yaml.contains("TAIL_PASSWORD: hunter2"));
}

#[test]
fn compose_omits_tail_env_when_absent() {
    let yaml = webhook::generate_compose("flow", "s", "fl-1", None, None, None);
    assert!(!yaml.contains("TAIL_URL"));
    assert!(!yaml.contains("TAIL_USER"));
    assert!(!yaml.contains("TAIL_PASSWORD"));
}

#[test]
fn caddy_fragment_proxies_to_webhook_service() {
    let frag = webhook::generate_caddy_fragment("webhooks.example.com");
    assert!(frag.starts_with("webhooks.example.com {"));
    assert!(frag.contains("reverse_proxy webhook:8080"));
}

#[test]
fn org_hooks_url() {
    use iron::config::RunnerScope;
    assert_eq!(
        webhook::hooks_base_url(&RunnerScope::Org, "flow-industries"),
        "https://api.github.com/orgs/flow-industries/hooks"
    );
}

#[test]
fn repo_hooks_url() {
    use iron::config::RunnerScope;
    assert_eq!(
        webhook::hooks_base_url(&RunnerScope::Repo, "flow-industries/iron"),
        "https://api.github.com/repos/flow-industries/iron/hooks"
    );
}

#[test]
fn script_is_valid_python_syntax() {
    if which("python3").is_none() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let script = webhook::generate_script();
    let path = std::env::temp_dir().join("iron-webhook-test-syntax.py");
    std::fs::write(&path, script).unwrap();
    let status = Command::new("python3")
        .arg("-m")
        .arg("py_compile")
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success(), "webhook.py failed py_compile");
}

struct Server {
    child: Child,
    port: u16,
    _guard: MutexGuard<'static, ()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn which(cmd: &str) -> Option<()> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.join(cmd).exists() {
            return Some(());
        }
    }
    None
}

fn pick_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn start_server(secret: &str) -> Server {
    let guard = SERVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let script = webhook::generate_script();
    let path = std::env::temp_dir().join(format!("iron-webhook-{}.py", std::process::id()));
    std::fs::write(&path, script).unwrap();

    let port = pick_port();
    let mut child = Command::new("python3")
        .arg(&path)
        .env("GITHUB_WEBHOOK_SECRET", secret)
        .env("BIND", format!("127.0.0.1:{port}"))
        .env_remove("TAIL_URL")
        .env_remove("TAIL_USER")
        .env_remove("TAIL_PASSWORD")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(_status)) = child.try_wait() {
            panic!("webhook server exited before binding port {port}");
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Server {
                child,
                port,
                _guard: guard,
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    panic!("webhook server failed to start on port {port}");
}

fn http_request(port: u16, raw: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    stream.flush().unwrap();
    let _ = stream.shutdown(Shutdown::Write);

    let mut buf = String::new();
    stream.read_to_string(&mut buf).unwrap();

    let status_line = buf.lines().next().unwrap_or("");
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);
    (code, buf)
}

fn signature(secret: &str, body: &str) -> String {
    let out = Command::new("python3")
        .arg("-c")
        .arg(
            "import hmac, hashlib, os; \
             print('sha256=' + hmac.new(os.environ['SECRET'].encode(), os.environ['BODY'].encode(), hashlib.sha256).hexdigest())",
        )
        .env("SECRET", secret)
        .env("BODY", body)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn script_health_endpoint_returns_ok() {
    if which("python3").is_none() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let server = start_server("test-secret");
    let (code, _) = http_request(
        server.port,
        "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(code, 200);
}

#[test]
fn script_rejects_bad_signature() {
    if which("python3").is_none() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let server = start_server("test-secret");
    let body = "{}";
    let raw = format!(
        "POST /github HTTP/1.1\r\n\
         Host: localhost\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         X-GitHub-Event: workflow_job\r\n\
         X-Hub-Signature-256: sha256=deadbeef\r\n\
         Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let (code, _) = http_request(server.port, &raw);
    assert_eq!(code, 401);
}

#[test]
fn script_accepts_valid_signature_and_ignores_non_workflow_job() {
    if which("python3").is_none() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let server = start_server("test-secret");
    let body = "{}";
    let sig = signature("test-secret", body);
    let raw = format!(
        "POST /github HTTP/1.1\r\n\
         Host: localhost\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         X-GitHub-Event: ping\r\n\
         X-Hub-Signature-256: {sig}\r\n\
         Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let (code, _) = http_request(server.port, &raw);
    assert_eq!(code, 204);
}

#[test]
fn script_accepts_workflow_job_with_valid_signature() {
    if which("python3").is_none() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let server = start_server("test-secret");
    let body = r#"{"action":"queued","workflow_job":{"name":"build","workflow_name":"CI","labels":["self-hosted","linux"],"runner_name":"ci"}}"#;
    let sig = signature("test-secret", body);
    let raw = format!(
        "POST /github HTTP/1.1\r\n\
         Host: localhost\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         X-GitHub-Event: workflow_job\r\n\
         X-Hub-Signature-256: {sig}\r\n\
         Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let (code, _) = http_request(server.port, &raw);
    assert_eq!(code, 202);
}
