use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuAction {
    Clean,
    Uninstall,
    Optimize,
    Analyze,
    Status,
    Purge,
    Installer,
    TouchID,
    History,
    Quit,
}

pub fn run_interactive_main_menu() -> io::Result<MainMenuAction> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let menu_options = [
        (MainMenuAction::Clean, "🧹 Clean", "Deep system & cache cleanup"),
        (MainMenuAction::Uninstall, "🗑️  Uninstall", "Remove apps and leftovers"),
        (MainMenuAction::Optimize, "⚡ Optimize", "Refresh caches & system services"),
        (MainMenuAction::Analyze, "📊 Analyze", "Visual disk space explorer"),
        (MainMenuAction::Status, "📈 Status", "Live system health dashboard"),
        (MainMenuAction::Purge, "📦 Purge", "Clean project build artifacts"),
        (MainMenuAction::Installer, "💿 Installer", "Remove raw installer files"),
        (MainMenuAction::TouchID, "🛡️  Touch ID", "Configure Touch ID for sudo"),
        (MainMenuAction::History, "📜 History", "View recent operation logs"),
        (MainMenuAction::Quit, "🚪 Quit", "Exit Pantau Cleaner"),
    ];

    let mut cursor = 0usize;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(10),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let banner = Paragraph::new(vec![
                Line::from(Span::styled(
                    "🐾 PANTAU CLEANER",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "Deep clean, optimize, and analyze your Mac with 100% native Rust.",
                    Style::default().fg(Color::Gray),
                )),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Pantau "));

            f.render_widget(banner, chunks[0]);

            let list_items: Vec<ListItem> = menu_options
                .iter()
                .enumerate()
                .map(|(idx, (_, title, desc))| {
                    let is_cursor = idx == cursor;
                    let prefix = if is_cursor { "▶ " } else { "  " };

                    let style = if is_cursor {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let line = Line::from(vec![
                        Span::styled(format!("{}{:<18} ", prefix, title), style),
                        Span::styled(format!("- {}", desc), Style::default().fg(Color::DarkGray)),
                    ]);

                    ListItem::new(line)
                })
                .collect();

            let list_widget = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(" Menu "));

            f.render_widget(list_widget, chunks[1]);

            let footer = Paragraph::new("↑/↓ / j/k: Navigate | Enter: Select | Q/Esc: Quit")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));

            f.render_widget(footer, chunks[2]);
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
                        if cursor + 1 < menu_options.len() {
                            cursor += 1;
                        }
                    }
                    KeyCode::Enter => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;
                        return Ok(menu_options[cursor].0);
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
    Ok(MainMenuAction::Quit)
}
