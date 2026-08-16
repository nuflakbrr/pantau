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
use std::path::Path;

use crate::cleaner::analyze::analyze_path;
use crate::cleaner::clean::format_bytes;

pub fn run_interactive_analyzer(target_path: &Path) -> io::Result<()> {
    let result = analyze_path(target_path);

    if !io::stdout().is_terminal() {
        println!("Path: {} | Total Size: {}", result.path, format_bytes(result.total_size_bytes));
        for entry in &result.entries {
            println!("  - {:<25} {:>5.1}% ({})", entry.name, entry.percent, format_bytes(entry.size_bytes));
        }
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = analyze_path(target_path);
    let mut cursor = 0usize;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Min(8),
                    Constraint::Length(7),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let header = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("Path: ", Style::default().fg(Color::Gray)),
                    Span::styled(&result.path, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("Total Size: ", Style::default().fg(Color::Gray)),
                    Span::styled(format_bytes(result.total_size_bytes), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  |  Total Files Scanned: {}", result.total_files), Style::default().fg(Color::DarkGray)),
                ]),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Disk Analysis "));

            f.render_widget(header, chunks[0]);

            // Top-level entries
            let entry_items: Vec<ListItem> = result
                .entries
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    let is_cursor = idx == cursor;
                    let prefix = if is_cursor { "▶ " } else { "  " };

                    let bar_len = (entry.percent / 4.0).clamp(0.0, 25.0) as usize;
                    let bar: String = "█".repeat(bar_len);
                    let empty_bar: String = "░".repeat(25 - bar_len);

                    let style = if is_cursor {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let line = Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(bar, Style::default().fg(Color::Blue)),
                        Span::styled(empty_bar, Style::default().fg(Color::DarkGray)),
                        Span::styled(format!(" {:>5.1}% | ", entry.percent), Style::default().fg(Color::Cyan)),
                        Span::styled(format!("{:<25} ", entry.name), style),
                        Span::styled(format_bytes(entry.size_bytes), Style::default().fg(Color::Magenta)),
                    ]);

                    ListItem::new(line)
                })
                .collect();

            let list_widget = List::new(entry_items)
                .block(Block::default().borders(Borders::ALL).title(" Directory Breakdown "));

            f.render_widget(list_widget, chunks[1]);

            // Top Large Files
            let large_items: Vec<ListItem> = result
                .large_files
                .iter()
                .take(5)
                .map(|file| {
                    let line = Line::from(vec![
                        Span::styled("  • ", Style::default().fg(Color::Red)),
                        Span::styled(format!("{:<30} ", file.name), Style::default().fg(Color::White)),
                        Span::styled(format_bytes(file.size_bytes), Style::default().fg(Color::Yellow)),
                        Span::styled(format!(" ({})", file.path), Style::default().fg(Color::DarkGray)),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let large_widget = List::new(large_items)
                .block(Block::default().borders(Borders::ALL).title(" Top Large Files (> 50MB) "));

            f.render_widget(large_widget, chunks[2]);

            let footer = Paragraph::new("↑/↓ / j/k: Navigate | Q/Esc: Back")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));

            f.render_widget(footer, chunks[3]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if cursor > 0 {
                            cursor -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if cursor + 1 < result.entries.len() {
                            cursor += 1;
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        break;
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
