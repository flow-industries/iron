pub const IMAGE: &str = "fluent/fluent-bit:4.0";

pub fn generate_compose(server_name: &str, tail_user: &str, tail_password: &str) -> String {
    format!(
        r#"services:
  fluent-bit:
    image: {IMAGE}
    container_name: fluent-bit
    volumes:
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./fluent-bit.conf:/fluent-bit/etc/fluent-bit.conf:ro
      - fluentbit-state:/var/lib/fluent-bit
    environment:
      FLOW_SERVER: {server_name}
      TAIL_USER: {tail_user}
      TAIL_PASSWORD: {tail_password}
    restart: always
    labels:
      - "flow.watch=false"

volumes:
  fluentbit-state:
"#
    )
}

pub fn generate_config(host: &str, port: u16, tls: bool, uri: &str) -> String {
    let tls_str = if tls { "On" } else { "Off" };
    format!(
        r"[SERVICE]
    Flush             5
    Daemon            Off
    Log_Level         info
    Parsers_File      /fluent-bit/etc/parsers.conf

[INPUT]
    Name              tail
    Path              /var/lib/docker/containers/*/*-json.log
    Path_Key          source
    Parser            docker
    Tag               docker.*
    Refresh_Interval  5
    Skip_Long_Lines   On
    DB                /var/lib/fluent-bit/tail-state.db
    Mem_Buf_Limit     32MB

[FILTER]
    Name              record_modifier
    Match             docker.*
    Record            server ${{FLOW_SERVER}}

[OUTPUT]
    Name              http
    Match             docker.*
    Host              {host}
    Port              {port}
    URI               {uri}
    Format            json
    Json_date_key     _timestamp
    Json_date_format  iso8601
    tls               {tls_str}
    http_user         ${{TAIL_USER}}
    http_passwd       ${{TAIL_PASSWORD}}
    compress          gzip
"
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
    let uri = "/api/default/app_logs/_json".to_string();
    Some((host, port, tls, uri))
}
