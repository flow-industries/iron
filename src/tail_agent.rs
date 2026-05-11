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

parsed, err = parse_json(string!(.message))
if err == null && is_object(parsed) {{
    . = merge(object!(parsed), ., deep: false)
    del(.message)
}}

if is_integer(.level) {{
    code = int!(.level)
    if code >= 60 {{
        .level = "fatal"
    }} else if code >= 50 {{
        .level = "error"
    }} else if code >= 40 {{
        .level = "warn"
    }} else if code >= 30 {{
        .level = "info"
    }} else if code >= 20 {{
        .level = "debug"
    }} else {{
        .level = "trace"
    }}
}}
'''

[transforms.drop_noise]
type = "filter"
inputs = ["tag"]

[transforms.drop_noise.condition]
type = "vrl"
source = '''
logger = to_string(.logger) ?? ""
msg = to_string(.message) ?? ""
uri = to_string(.request_uri) ?? ""

is_caddy_access = starts_with(logger, "http.log.access")
is_nginx_health = contains(msg, " /health ") || contains(msg, " /healthz ")
is_health_req = uri == "/health" || uri == "/healthz"

!(is_caddy_access || is_nginx_health || is_health_req)
'''

[sinks.observe]
type = "http"
inputs = ["drop_noise"]
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
    let uri = "/api/default/app_logs/_multi".to_string();
    Some((host, port, tls, uri))
}
