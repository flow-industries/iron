use anyhow::Result;
use comfy_table::{
    Cell, CellAlignment, Color, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL_CONDENSED,
};
use console::style;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use std::collections::HashMap;
use std::fmt::Write;
use std::io::Write as IoWrite;
use std::time::Duration;

use crate::config::Fleet;
use crate::ssh::SshPool;
use crate::ui;

pub struct Columns {
    pub image: bool,
    pub ports: bool,
    pub size: bool,
}

pub async fn run(
    fleet: &Fleet,
    server_filter: Option<&str>,
    follow: bool,
    cols: Columns,
) -> Result<()> {
    let filtered: HashMap<String, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| server_filter.is_none() || server_filter == Some(name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if filtered.is_empty() {
        anyhow::bail!("No matching server found");
    }

    let sp = ui::spinner("Connecting...");
    let pool = SshPool::connect(&filtered).await?;
    sp.finish_and_clear();

    if follow {
        follow_loop(&pool, &filtered, &cols).await?;
        pool.close().await?;
    } else {
        print_status(&pool, &filtered, &cols).await?;
        pool.close().await?;
    }
    Ok(())
}

async fn follow_loop(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
    cols: &Columns,
) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    print!("\x1b[?25l");

    let result = follow_inner(pool, servers, cols).await;

    crossterm::terminal::disable_raw_mode()?;
    print!("\x1b[?25h");
    std::io::stdout().flush()?;

    result
}

async fn follow_inner(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
    cols: &Columns,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut show_esc_hint = false;

    print!("\x1b[2J");
    std::io::stdout().flush()?;

    loop {
        let buf = render_status(pool, servers, cols).await?;
        let hint = if show_esc_hint {
            format!("\n{}", style("(press esc to quit)").dim())
        } else {
            String::new()
        };
        let cleared = buf.replace('\n', "\x1b[K\n");
        print!("\x1b[H{cleared}{hint}\x1b[K\x1b[J");
        std::io::stdout().flush()?;

        loop {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(1)) => break,
                event = events.next() => {
                    match event {
                        Some(Ok(Event::Key(KeyEvent { code: KeyCode::Esc, .. }))) => {
                            if show_esc_hint {
                                return Ok(());
                            }
                            show_esc_hint = true;
                            break;
                        }
                        Some(Ok(Event::Key(KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers,
                            ..
                        }))) if modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

const DOCKER_CMD: &str = "\
docker ps -a --format '{{.Names}}\t{{.Status}}\t{{.Image}}\t{{.Ports}}\t{{.Size}}' 2>/dev/null; \
echo '---STATS---'; \
docker stats --no-stream --format '{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}' 2>/dev/null";

struct ContainerPs {
    status: String,
    image: String,
    ports: String,
    size: String,
}

struct ContainerStats {
    cpu: String,
    mem: String,
}

fn parse_output(
    output: &str,
) -> (
    HashMap<String, ContainerPs>,
    HashMap<String, ContainerStats>,
    Vec<String>,
) {
    let mut ps_map = HashMap::new();
    let mut stats_map = HashMap::new();
    let mut order = Vec::new();
    let mut in_stats = false;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        if line == "---STATS---" {
            in_stats = true;
            continue;
        }
        if in_stats {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() == 3 {
                stats_map.insert(
                    parts[0].to_string(),
                    ContainerStats {
                        cpu: parts[1].to_string(),
                        mem: parts[2].to_string(),
                    },
                );
            }
        } else {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() == 5 {
                order.push(parts[0].to_string());
                ps_map.insert(
                    parts[0].to_string(),
                    ContainerPs {
                        status: parts[1].to_string(),
                        image: parts[2].to_string(),
                        ports: parts[3].to_string(),
                        size: parts[4].to_string(),
                    },
                );
            }
        }
    }

    (ps_map, stats_map, order)
}

fn build_table(
    ps_map: &HashMap<String, ContainerPs>,
    stats_map: &HashMap<String, ContainerStats>,
    order: &[String],
    cols: &Columns,
) -> Table {
    let mut header: Vec<Cell> = vec![
        Cell::new("Container"),
        Cell::new("Status"),
        Cell::new("CPU"),
        Cell::new("Memory"),
    ];
    if cols.image {
        header.push(Cell::new("Image"));
    }
    if cols.ports {
        header.push(Cell::new("Ports"));
    }
    if cols.size {
        header.push(Cell::new("Size"));
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);

    for name in order {
        let Some(ps) = ps_map.get(name) else {
            continue;
        };
        let stats = stats_map.get(name);
        let status_color = if ps.status.starts_with("Up") {
            Color::Green
        } else {
            Color::Red
        };

        let cpu = stats.map_or("—", |s| &s.cpu);
        let mem = stats.map_or("—", |s| &s.mem);

        let mut row: Vec<Cell> = vec![
            Cell::new(name),
            Cell::new(&ps.status).fg(status_color),
            Cell::new(cpu).set_alignment(CellAlignment::Right),
            Cell::new(mem).set_alignment(CellAlignment::Right),
        ];
        if cols.image {
            row.push(Cell::new(&ps.image));
        }
        if cols.ports {
            row.push(Cell::new(&ps.ports));
        }
        if cols.size {
            row.push(Cell::new(&ps.size).set_alignment(CellAlignment::Right));
        }

        table.add_row(row);
    }

    table
}

async fn render_status(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
    cols: &Columns,
) -> Result<String> {
    let mut buf = String::new();
    for name in servers.keys() {
        writeln!(
            buf,
            "\n{}",
            style(format!("Server: {name}")).bold().underlined()
        )?;

        let output = pool.exec(name, DOCKER_CMD).await?;
        let (ps_map, stats_map, order) = parse_output(&output);
        let table = build_table(&ps_map, &stats_map, &order, cols);
        writeln!(buf, "{table}")?;
    }
    Ok(buf)
}

async fn print_status(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
    cols: &Columns,
) -> Result<()> {
    for name in servers.keys() {
        ui::header(&format!("Server: {name}"));

        let output = pool.exec(name, DOCKER_CMD).await?;
        let (ps_map, stats_map, order) = parse_output(&output);
        let table = build_table(&ps_map, &stats_map, &order, cols);
        println!("{table}");
    }
    Ok(())
}
