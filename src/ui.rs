use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    #[allow(clippy::unwrap_used)]
    let style = ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner:.cyan} {msg}")
        .unwrap();
    pb.set_style(style);
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

pub fn success(msg: &str) {
    println!("{} {}", style("✓").green().bold(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", style("✗").red().bold(), msg);
}

pub fn header(msg: &str) {
    println!("\n{}", style(msg).bold().underlined());
}

pub fn confirm(msg: &str) -> bool {
    eprint!("{} {} ", style("?").yellow().bold(), msg);
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

pub fn prompt(label: &str) -> Option<String> {
    eprint!("{} {} ", style("?").yellow().bold(), label);
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return None;
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
