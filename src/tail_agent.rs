pub const IMAGE: &str = "timberio/vector:0.49.0-alpine";

pub fn generate_compose(server_name: &str, tail_user: &str, tail_password: &str) -> String {
    format!(
        r#"services:
  tail-agent:
    image: {IMAGE}
    container_name: tail-agent
    command: ["--config", "/etc/vector/vector.toml"]
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./vector.toml:/etc/vector/vector.toml:ro
      - tail-agent-state:/var/lib/vector
    environment:
      FLOW_SERVER: {server_name}
      TAIL_USER: {tail_user}
      TAIL_PASSWORD: {tail_password}
    restart: always
    labels:
      - "flow.watch=false"

volumes:
  tail-agent-state:
"#
    )
}

pub fn generate_config(host: &str, port: u16, tls: bool, uri: &str) -> String {
    let scheme = if tls { "https" } else { "http" };
    format!(
        r#"data_dir = "/var/lib/vector"

[sources.docker]
type = "docker_logs"
exclude_containers = ["tail-agent", "observe-"]

[transforms.tag]
type = "remap"
inputs = ["docker"]
source = '''
.server = get_env_var("FLOW_SERVER") ?? "?"
'''

[sinks.observe]
type = "http"
inputs = ["tag"]
uri = "{scheme}://{host}:{port}{uri}"
encoding.codec = "json"
framing.method = "newline_delimited"
compression = "gzip"
auth.strategy = "basic"
auth.user = "${{TAIL_USER}}"
auth.password = "${{TAIL_PASSWORD}}"
"#
    )
}

pub fn parse_tail_url(tail_url: &str) -> Option<(String, u16, bool, String)> {
    let (scheme, rest) = tail_url.split_once("://")?;
    let tls = match scheme {
        "https" => true,
        "http" => false,
        _ => return None,
    };
    let (host_port, _) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        (h.to_string(), p.parse().ok()?)
    } else {
        (host_port.to_string(), if tls { 443 } else { 80 })
    };
    let uri = "/api/default/app_logs/_bulk".to_string();
    Some((host, port, tls, uri))
}
