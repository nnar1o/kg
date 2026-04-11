use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use kg::{FindMode, GraphFile, Node};

#[derive(Debug, Parser)]
#[command(name = "kg-tui", about = "Interactive graph browser (async search)")]
struct Args {
    graph: String,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    include_features: bool,
    #[arg(long, value_enum, default_value_t = FindModeArg::Fuzzy)]
    mode: FindModeArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FindModeArg {
    Fuzzy,
    Bm25,
}

impl FindModeArg {
    fn as_find_mode(self) -> FindMode {
        match self {
            FindModeArg::Fuzzy => FindMode::Fuzzy,
            FindModeArg::Bm25 => FindMode::Bm25,
        }
    }
}

#[derive(Debug)]
struct SearchRequest {
    seq: u64,
    query: String,
}

#[derive(Debug)]
struct SearchResult {
    seq: u64,
    results: Vec<Node>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let cwd = std::env::current_dir()?;
    let graph_root = kg::default_graph_root(&cwd);
    let path = kg::resolve_graph_path(&cwd, &graph_root, &args.graph)?;
    let graph = GraphFile::load(&path)
        .with_context(|| format!("failed to load graph: {}", path.display()))?;

    let graph = Arc::new(graph);
    let (req_tx, req_rx) = mpsc::channel::<SearchRequest>();
    let (res_tx, res_rx) = mpsc::channel::<SearchResult>();
    let search_graph = Arc::clone(&graph);
    let search_limit = args.limit;
    let search_include = args.include_features;
    let search_mode = args.mode.as_find_mode();

    std::thread::spawn(move || {
        while let Ok(req) = req_rx.recv() {
            let query = req.query.trim();
            let results = if query.is_empty() {
                Vec::new()
            } else {
                kg::output::find_nodes(
                    &search_graph,
                    query,
                    search_limit,
                    search_include,
                    false,
                    search_mode,
                )
            };
            let _ = res_tx.send(SearchResult {
                seq: req.seq,
                results,
            });
        }
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut query = String::new();
    let mut results: Vec<Node> = Vec::new();
    let mut list_state = ListState::default();
    list_state.select(Some(0));
    let mut seq = 0u64;
    let mut last_seq = 0u64;
    let mut pending_search = false;
    let mut last_input = Instant::now();
    let debounce = Duration::from_millis(200);

    loop {
        while let Ok(res) = res_rx.try_recv() {
            if res.seq >= last_seq {
                last_seq = res.seq;
                results = res.results;
                list_state.select(Some(0));
            }
        }

        if pending_search && last_input.elapsed() >= debounce {
            seq += 1;
            let _ = req_tx.send(SearchRequest {
                seq,
                query: query.clone(),
            });
            pending_search = false;
        }

        terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(frame.area());

            let header = Paragraph::new(format!(
                "Graph: {} | mode: {:?} | query: {}",
                args.graph,
                args.mode,
                if query.is_empty() {
                    "(type to search)"
                } else {
                    &query
                }
            ))
            .block(Block::default().borders(Borders::ALL).title("kg-tui"));
            frame.render_widget(header, layout[0]);

            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(layout[1]);

            let items: Vec<ListItem> = results
                .iter()
                .map(|node| ListItem::new(format!("{} | {}", node.id, node.name)))
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Results"))
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");
            frame.render_stateful_widget(list, body[0], &mut list_state);

            let detail = if let Some(idx) = list_state.selected() {
                results.get(idx)
            } else {
                None
            };
            let detail_lines = detail
                .map(|node| render_node_detail(&graph, node))
                .unwrap_or_else(|| {
                    vec![Line::from(Span::styled(
                        "No selection",
                        Style::default().fg(Color::DarkGray),
                    ))]
                });

            let detail = Paragraph::new(detail_lines)
                .block(Block::default().borders(Borders::ALL).title("Detail"))
                .wrap(Wrap { trim: true });
            frame.render_widget(detail, body[1]);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match (code, modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => break,
                    (KeyCode::Esc, _) => {
                        query.clear();
                        pending_search = true;
                        last_input = Instant::now();
                    }
                    (KeyCode::Backspace, _) => {
                        query.pop();
                        pending_search = true;
                        last_input = Instant::now();
                    }
                    (KeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        query.push(ch);
                        pending_search = true;
                        last_input = Instant::now();
                    }
                    (KeyCode::Up, _) => {
                        let next = match list_state.selected() {
                            Some(idx) if idx > 0 => idx - 1,
                            _ => 0,
                        };
                        list_state.select(Some(next));
                    }
                    (KeyCode::Down, _) => {
                        let next = match list_state.selected() {
                            Some(idx) => (idx + 1).min(results.len().saturating_sub(1)),
                            None => 0,
                        };
                        list_state.select(Some(next));
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn render_node_detail(graph: &GraphFile, node: &Node) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("{} | {}", node.id, node.name),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if !node.properties.description.is_empty() {
        lines.push(Line::from(node.properties.description.clone()));
    }
    if !node.properties.alias.is_empty() {
        lines.push(Line::from(format!(
            "aka: {}",
            node.properties.alias.join(", ")
        )));
    }
    if !node.properties.key_facts.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "facts",
            Style::default().fg(Color::LightBlue),
        )));
        for fact in node.properties.key_facts.iter().take(5) {
            lines.push(Line::from(format!("- {fact}")));
        }
    }

    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();
    for edge in &graph.edges {
        if edge.source_id == node.id {
            outgoing.push(edge);
        }
        if edge.target_id == node.id {
            incoming.push(edge);
        }
    }
    if !outgoing.is_empty() || !incoming.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "connections",
            Style::default().fg(Color::LightBlue),
        )));
        for edge in outgoing.iter().take(5) {
            lines.push(Line::from(format!(
                "-> {} {}",
                edge.relation, edge.target_id
            )));
        }
        for edge in incoming.iter().take(5) {
            lines.push(Line::from(format!(
                "<- {} {}",
                edge.relation, edge.source_id
            )));
        }
    }

    let notes: Vec<_> = graph
        .notes
        .iter()
        .filter(|note| note.node_id == node.id)
        .collect();
    if !notes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "notes",
            Style::default().fg(Color::LightBlue),
        )));
        for note in notes.iter().take(5) {
            let mut line = String::new();
            if !note.created_at.is_empty() {
                line.push_str(&note.created_at);
                line.push(' ');
            }
            if !note.author.is_empty() {
                line.push_str(&note.author);
                line.push_str(": ");
            }
            line.push_str(&note.body);
            lines.push(Line::from(truncate(&line, 120)));
        }
    }

    lines
}

fn truncate(value: &str, max_len: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_len {
        return value.to_owned();
    }
    let truncated: String = value.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}
