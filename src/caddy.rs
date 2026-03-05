use crate::config::ResolvedApp;

pub fn generate(app: &ResolvedApp) -> Option<String> {
    let routing = app.routing.as_ref()?;
    let port = app.port?;

    let health_path = routing.health_path.as_deref().unwrap_or("/health");
    let health_interval = routing.health_interval.as_deref().unwrap_or("5s");

    let mut out = String::new();
    for route in &routing.routes {
        out.push_str(&format!(
            "{route} {{\n    reverse_proxy {name}:{port} {{\n        health_uri {health_path}\n        health_interval {health_interval}\n        lb_try_duration 10s\n    }}\n}}\n",
            route = route,
            name = app.name,
            port = port,
            health_path = health_path,
            health_interval = health_interval,
        ));
    }

    Some(out)
}
