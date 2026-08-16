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

#[derive(Debug, Clone)]
pub struct SelectableItem {
    pub id: usize,
    pub title: String,
    pub detail: String,
    pub size_formatted: String,
    pub is_selected: bool,
    pub is_disabled: bool,
}

pub fn run_interactive_selector(
    header_title: &str,
    mut items: Vec<SelectableItem>,
) -> io::Result<Option<Vec<usize>>> {
    if !io::stdout().is_terminal() {
        println!("{}:", header_title);
        for item in &items {
            println!(
                "  [{}] {:<30} {:<12} {}",
                if item.is_selected { "✓" } else { " " },
                item.title,
                item.size_formatted,
                item.detail
            );
        }
        let selected: Vec<usize> = items
            .into_iter()
            .filter(|i| i.is_selected)
            .map(|i| i.id)
            .collect();
        return Ok(Some(selected));
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut cursor = 0usize;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let selected_count = items.iter().filter(|i| i.is_selected).count();
            let header = Paragraph::new(format!(
                "{} ({} of {} selected)",
                header_title,
                selected_count,
                items.len()
            ))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).title(" Pantau Cleaner "));

            f.render_widget(header, chunks[0]);

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let is_cursor = idx == cursor;
                    let check = if item.is_selected { "[✓]" } else { "[ ]" };
                    let prefix = if is_cursor { "▶ " } else { "  " };

                    let style = if is_cursor {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if item.is_disabled {
                        Style::default().fg(Color::DarkGray)
                    } else if item.is_selected {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let line = Line::from(vec![
                        Span::styled(format!("{}{} ", prefix, check), style),
                        Span::styled(format!("{:<30} ", item.title), style),
                        Span::styled(format!("{:<15} ", item.size_formatted), Style::default().fg(Color::Magenta)),
                        Span::styled(item.detail.clone(), Style::default().fg(Color::Gray)),
                    ]);

                    ListItem::new(line)
                })
                .collect();

            let list_widget = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(" Items "));

            f.render_widget(list_widget, chunks[1]);

            let footer = Paragraph::new("↑/↓: Navigate | Space: Toggle | A: Toggle All | Enter: Confirm | Q/Esc: Cancel")
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
                        if cursor + 1 < items.len() {
                            cursor += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(item) = items.get_mut(cursor) {
                            if !item.is_disabled {
                                item.is_selected = !item.is_selected;
                            }
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let any_unselected = items.iter().any(|i| !i.is_selected && !i.is_disabled);
                        for item in &mut items {
                            if !item.is_disabled {
                                item.is_selected = any_unselected;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;
                        let selected_ids: Vec<usize> = items
                            .into_iter()
                            .filter(|i| i.is_selected)
                            .map(|i| i.id)
                            .collect();
                        return Ok(Some(selected_ids));
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
    Ok(None)
}
