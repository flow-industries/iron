use crate::config::ResolvedApp;

pub fn generate(app: &ResolvedApp) -> Option<String> {
    let routing = app.routing.as_ref()?;
    let port = app.port?;

    let health_block = if let Some(health_path) = &routing.health_path {
        let interval = routing.health_interval.as_deref().unwrap_or("5s");
        format!(
            " {{\n        health_uri {health_path}\n        health_interval {interval}\n        lb_try_duration 10s\n    }}"
        )
    } else {
        String::new()
    };

    let mut out = String::new();
    for domain in &routing.domains {
        out.push_str(&format!(
            "{domain} {{\n    encode zstd gzip\n    reverse_proxy {name}:{port}{health_block}\n}}\n",
            domain = domain,
            name = app.name,
            port = port,
            health_block = health_block,
        ));
    }

    Some(out)
}
