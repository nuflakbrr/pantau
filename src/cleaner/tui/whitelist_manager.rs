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

use crate::cleaner::config::{get_all_discoverable_cache_items, CleanerConfig};

struct WhitelistEntry {
    display_name: String,
    pattern: String,
    category: String,
    is_selected: bool,
    is_custom: bool,
}

pub fn run_interactive_whitelist_manager() -> io::Result<()> {
    let config = CleanerConfig::new();
    let current_patterns = config.load_whitelist();

    let mut entries: Vec<WhitelistEntry> = Vec::new();

    // 1. Add all dynamically discovered cache items & presets
    let discovered = get_all_discoverable_cache_items();
    for item in discovered {
        let is_selected = current_patterns.iter().any(|p| {
            let p_trim = p.trim_end_matches("/*").trim_end_matches('*');
            let i_trim = item.pattern.trim_end_matches("/*").trim_end_matches('*');
            p == &item.pattern || p_trim == i_trim || item.pattern.starts_with(p_trim) || p.starts_with(i_trim)
        });
        entries.push(WhitelistEntry {
            display_name: item.display_name,
            pattern: item.pattern,
            category: item.category,
            is_selected,
            is_custom: false,
        });
    }

    // 2. Add custom patterns from config that aren't in predefined list
    for p in &current_patterns {
        let exists = entries.iter().any(|e| &e.pattern == p);
        if !exists {
            entries.push(WhitelistEntry {
                display_name: format!("Custom: {}", p),
                pattern: p.clone(),
                category: "custom".to_string(),
                is_selected: true,
                is_custom: true,
            });
        }
    }

    // 3. Sort so selected items appear at the top
    entries.sort_by(|a, b| b.is_selected.cmp(&a.is_selected));

    let config_display = config
        .whitelist_file
        .to_string_lossy()
        .replace(&std::env::var("HOME").unwrap_or_default(), "~");

    if !io::stdout().is_terminal() {
        println!("Whitelist Manager (Non-interactive mode)");
        println!("Config: {}\n", config_display);
        for e in &entries {
            println!(
                "  {} {}",
                if e.is_selected { "●" } else { "○" },
                e.display_name
            );
        }
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut cursor = 0usize;

    let mut saved = false;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Min(6),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let selected_count = entries.iter().filter(|i| i.is_selected).count();
            let header_text = vec![
                Line::from(vec![
                    Span::styled("Whitelist Manager, ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    Span::styled("Select caches to protect", Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled(format!("Edit: {}  ", config_display), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{}/{} selected", selected_count, entries.len()),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];

            let header = Paragraph::new(header_text)
                .block(Block::default().borders(Borders::ALL).title(" 🛡️  Pantau Protection "));
            f.render_widget(header, chunks[0]);

            // Scroll window calculation
            let visible_height = chunks[1].height.saturating_sub(2) as usize;
            let offset = if cursor >= visible_height {
                cursor - visible_height + 1
            } else {
                0
            };

            let list_items: Vec<ListItem> = entries
                .iter()
                .skip(offset)
                .take(visible_height)
                .enumerate()
                .map(|(rel_idx, item)| {
                    let idx = offset + rel_idx;
                    let is_cursor = idx == cursor;
                    let bullet = if item.is_selected { "●" } else { "○" };
                    let prefix = if is_cursor { "➤ " } else { "  " };

                    let bullet_style = if item.is_selected {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let title_style = if is_cursor {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else if item.is_selected {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    let line = Line::from(vec![
                        Span::styled(prefix, if is_cursor { Style::default().fg(Color::Cyan) } else { Style::default() }),
                        Span::styled(format!("{} ", bullet), bullet_style),
                        Span::styled(item.display_name.clone(), title_style),
                    ]);

                    ListItem::new(line)
                })
                .collect();

            let list_widget = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(" Caches & Directories "));
            f.render_widget(list_widget, chunks[1]);

            let footer = Paragraph::new("↑↓ | Space Toggle | A Toggle All | Enter Save | Q Cancel")
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
                        if cursor + 1 < entries.len() {
                            cursor += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(item) = entries.get_mut(cursor) {
                            item.is_selected = !item.is_selected;
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        let any_unselected = entries.iter().any(|i| !i.is_selected);
                        for item in &mut entries {
                            item.is_selected = any_unselected;
                        }
                    }
                    KeyCode::Enter => {
                        saved = true;
                        break;
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

    if saved {
        let selected_patterns: Vec<String> = entries
            .into_iter()
            .filter(|e| e.is_selected)
            .map(|e| e.pattern)
            .collect();
        let count = selected_patterns.len();
        let _ = config.save_whitelist(&selected_patterns);

        println!("\x1b[32m✓ Whitelist Updated\x1b[0m");
        println!("  Protected {} caches & patterns", count);
        println!("  Config: \x1b[2m{}\x1b[0m\n", config_display);
    } else {
        println!("\x1b[2mCancelled, no changes saved\x1b[0m\n");
    }

    Ok(())
}
