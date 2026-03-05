use std::path::Path;

use anyhow::Result;

use crate::ui;

const TEMPLATE: &str = r#"# domain = "example.com"
#
# [servers.my-server]
# host = "my-server.example.com"
#
# [apps.my-app]
# image = "ghcr.io/org/app:latest"
# servers = ["my-server"]
# port = 3000
#
# [apps.my-app.routing]
# routes = ["app.example.com"]
"#;

pub fn run(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);

    if path.exists() {
        ui::success(&format!("{config_path} already exists"));
        return Ok(());
    }

    std::fs::write(path, TEMPLATE)?;
    ui::success(&format!("Created {config_path}"));
    Ok(())
}
