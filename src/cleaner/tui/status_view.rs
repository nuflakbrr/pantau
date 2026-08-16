use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use crate::cleaner::clean::format_bytes;
use crate::cleaner::status::collect_system_metrics;

pub fn run_interactive_status_dashboard() -> io::Result<()> {
    if !io::stdout().is_terminal() {
        let snap = collect_system_metrics();
        println!("Pantau Status: Health Score {} | Host: {} | OS: {}", snap.health_score, snap.hostname, snap.os_version);
        println!("  CPU: {:.1}% | Memory: {:.1}% | Disk: {:.1}%", snap.cpu_total_pct, snap.memory_used_pct, snap.disk_used_pct);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(1500);
    let mut snapshot = collect_system_metrics();

    loop {
        if last_tick.elapsed() >= tick_rate {
            snapshot = collect_system_metrics();
            last_tick = Instant::now();
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Length(6),
                    Constraint::Min(6),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let score_color = if snapshot.health_score > 80 {
                Color::Green
            } else if snapshot.health_score > 60 {
                Color::Yellow
            } else {
                Color::Red
            };

            let header = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Host: ", Style::default().fg(Color::Gray)),
                    Span::styled(&snapshot.hostname, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled("  |  OS: ", Style::default().fg(Color::Gray)),
                    Span::styled(&snapshot.os_version, Style::default().fg(Color::White)),
                    Span::styled("  |  Health Score: ", Style::default().fg(Color::Gray)),
                    Span::styled(format!("● {}", snapshot.health_score), Style::default().fg(score_color).add_modifier(Modifier::BOLD)),
                ]),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Live System Health "));

            f.render_widget(header, chunks[0]);

            // Metrics row: CPU, Memory, Disk
            let cpu_bar_len = (snapshot.cpu_total_pct / 5.0).clamp(0.0, 20.0) as usize;
            let mem_bar_len = (snapshot.memory_used_pct / 5.0).clamp(0.0, 20.0) as usize;
            let disk_bar_len = (snapshot.disk_used_pct / 5.0).clamp(0.0, 20.0) as usize;

            let metrics_lines = vec![
                Line::from(vec![
                    Span::styled("⚙  CPU    ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled("█".repeat(cpu_bar_len), Style::default().fg(Color::Green)),
                    Span::styled("░".repeat(20 - cpu_bar_len), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {:>5.1}%", snapshot.cpu_total_pct), Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("▦  Memory ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                    Span::styled("█".repeat(mem_bar_len), Style::default().fg(Color::Magenta)),
                    Span::styled("░".repeat(20 - mem_bar_len), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {:>5.1}% ({} / {})", snapshot.memory_used_pct, format_bytes(snapshot.memory_used_bytes), format_bytes(snapshot.memory_total_bytes)), Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("▤  Disk   ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                    Span::styled("█".repeat(disk_bar_len), Style::default().fg(Color::Blue)),
                    Span::styled("░".repeat(20 - disk_bar_len), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!(" {:>5.1}% ({} / {})", snapshot.disk_used_pct, format_bytes(snapshot.disk_used_bytes), format_bytes(snapshot.disk_total_bytes)), Style::default().fg(Color::Cyan)),
                ]),
            ];

            let metrics_widget = Paragraph::new(metrics_lines)
                .block(Block::default().borders(Borders::ALL).title(" Core Resources "));

            f.render_widget(metrics_widget, chunks[1]);

            // Top Processes list
            let proc_items: Vec<ListItem> = snapshot
                .top_processes
                .iter()
                .map(|p| {
                    let line = Line::from(vec![
                        Span::styled(format!("  PID {:<6} ", p.pid), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{:<25} ", p.name), Style::default().fg(Color::White)),
                        Span::styled(format!("CPU: {:>5.1}%  ", p.cpu_percent), Style::default().fg(Color::Yellow)),
                        Span::styled(format!("RSS: {}", format_bytes(p.mem_bytes)), Style::default().fg(Color::Cyan)),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let proc_widget = List::new(proc_items)
                .block(Block::default().borders(Borders::ALL).title(" Top Active Processes "));

            f.render_widget(proc_widget, chunks[2]);

            let footer = Paragraph::new("Auto-refreshes every 1.5s | Q/Esc: Quit")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));

            f.render_widget(footer, chunks[3]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
