use crate::config::{DeployStrategy, ResolvedApp};

pub fn generate(app: &ResolvedApp, network: &str) -> String {
    let mut out = String::from("services:\n");

    out.push_str(&format!("  {}:\n", app.name));
    out.push_str(&format!("    image: {}\n", app.image));

    if !app.env.is_empty() {
        out.push_str("    env_file:\n");
        out.push_str("      - .env\n");
    }

    out.push_str("    restart: always\n");

    if !app.ports.is_empty() {
        out.push_str("    ports:\n");
        for p in &app.ports {
            if p.protocol == "tcp" {
                out.push_str(&format!("      - \"{}:{}\"\n", p.external, p.internal));
            } else {
                out.push_str(&format!(
                    "      - \"{}:{}/{}\"\n",
                    p.external, p.internal, p.protocol
                ));
            }
        }
    }

    if app.routing.is_some() {
        out.push_str("    networks:\n");
        out.push_str(&format!("      - {network}\n"));
    }

    let strategy = if app.deploy_strategy == DeployStrategy::Recreate {
        "recreate"
    } else {
        "rolling"
    };
    if !app.volumes.is_empty() {
        out.push_str("    volumes:\n");
        for vol in &app.volumes {
            out.push_str(&format!("      - {vol}\n"));
        }
    }

    out.push_str("    labels:\n");
    out.push_str("      - \"flow.watch=true\"\n");
    out.push_str(&format!("      - \"flow.strategy={strategy}\"\n"));

    if let Some(ref routing) = app.routing {
        if let Some(ref health_path) = routing.health_path {
            let port = app.port.unwrap_or(3000);
            out.push_str("    healthcheck:\n");
            let url = format!("http://127.0.0.1:{port}{health_path}");
            out.push_str(&format!(
                "      test: [\"CMD-SHELL\", \"curl -sf {url} > /dev/null || wget --spider -q {url}\"]\n"
            ));
            out.push_str("      interval: 10s\n");
            out.push_str("      timeout: 5s\n");
            out.push_str("      retries: 3\n");
            out.push_str("      start_period: 15s\n");
        }
    }

    let healthy_sidecars: Vec<_> = app
        .services
        .iter()
        .filter(|s| s.healthcheck.is_some())
        .collect();
    if !healthy_sidecars.is_empty() {
        out.push_str("    depends_on:\n");
        for svc in &healthy_sidecars {
            out.push_str(&format!("      {}:\n", svc.name));
            out.push_str("        condition: service_healthy\n");
        }
    }

    for svc in &app.services {
        out.push_str(&format!("\n  {}:\n", svc.name));
        out.push_str(&format!("    image: {}\n", svc.image));

        out.push_str("    labels:\n");
        out.push_str("      - \"flow.watch=false\"\n");

        if !svc.env.is_empty() {
            out.push_str("    env_file:\n");
            out.push_str("      - .env\n");
        }

        if !svc.volumes.is_empty() {
            out.push_str("    volumes:\n");
            for vol in &svc.volumes {
                out.push_str(&format!("      - {vol}\n"));
            }
        }

        out.push_str("    restart: always\n");

        if app.routing.is_some() {
            out.push_str("    networks:\n");
            out.push_str(&format!("      - {network}\n"));
        }

        if let Some(ref hc) = svc.healthcheck {
            out.push_str("    healthcheck:\n");
            out.push_str(&format!("      test: [\"CMD-SHELL\", \"{hc}\"]\n"));
            out.push_str("      interval: 5s\n");
            out.push_str("      timeout: 5s\n");
            out.push_str("      retries: 5\n");
        }

        if let Some(ref dep) = svc.depends_on {
            out.push_str("    depends_on:\n");
            let dep_has_healthcheck = app
                .services
                .iter()
                .any(|s| s.name == *dep && s.healthcheck.is_some());
            if dep_has_healthcheck {
                out.push_str(&format!("      {dep}:\n"));
                out.push_str("        condition: service_healthy\n");
            } else {
                out.push_str(&format!("      - {dep}\n"));
            }
        }
    }

    if app.routing.is_some() {
        out.push_str("\nnetworks:\n");
        out.push_str(&format!("  {network}:\n"));
        out.push_str("    external: true\n");
    }

    let mut named_volumes: Vec<String> = Vec::new();
    let collect_named = |vol: &str, named: &mut Vec<String>| {
        if let Some(name) = vol.split(':').next() {
            let is_named_volume = !name.contains('/') && !name.starts_with('.');
            if is_named_volume && !named.contains(&name.to_string()) {
                named.push(name.to_string());
            }
        }
    };
    for vol in &app.volumes {
        collect_named(vol, &mut named_volumes);
    }
    for svc in &app.services {
        for vol in &svc.volumes {
            collect_named(vol, &mut named_volumes);
        }
    }
    if !named_volumes.is_empty() {
        out.push_str("\nvolumes:\n");
        for vol in &named_volumes {
            out.push_str(&format!("  {vol}:\n"));
        }
    }

    out
}

// Persisted to fleet.env.toml for `iron remove` revocation; must never reach app containers.
const HIDDEN_FROM_CONTAINER: &[&str] = &["R2_ACCESS_KEY_TOKEN_ID"];

pub fn generate_env(app: &ResolvedApp) -> String {
    let mut out = String::new();
    let mut all_vars: std::collections::HashMap<String, String> = app.env.clone();

    for svc in &app.services {
        for (k, v) in &svc.env {
            all_vars.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    for key in HIDDEN_FROM_CONTAINER {
        all_vars.remove(*key);
    }

    let mut keys: Vec<_> = all_vars.keys().collect();
    keys.sort();
    for key in keys {
        out.push_str(&format!("{}={}\n", key, all_vars[key]));
    }

    out
}
