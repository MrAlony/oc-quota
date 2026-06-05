use crate::state::SharedState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, List, ListItem, Paragraph, Gauge, Tabs, Table, Row, Cell},
    Frame,
};

pub fn draw(f: &mut Frame, state: SharedState, list_state: &mut ratatui::widgets::ListState) {
    let state = state.read();
    
    // Define Theme Colors
    let color_primary = Color::Cyan;
    let color_secondary = Color::LightMagenta;
    let color_success = Color::LightGreen;
    let color_error = Color::LightRed;
    let color_warning = Color::Yellow;
    let color_text = Color::White;
    let color_muted = Color::DarkGray;

    let size = f.area();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Tabs
            Constraint::Min(10),   // Main View
            Constraint::Length(3), // Footer
        ])
        .split(size);

    // --- Header ---
    let title = Paragraph::new(Line::from(vec![
        Span::styled(" 🚀 OC-QUOTA Unified Proxy ", Style::default().fg(color_primary).add_modifier(Modifier::BOLD)),
        Span::styled("v1.2.2 ", Style::default().fg(color_muted)),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Double).border_style(Style::default().fg(color_primary)));
    f.render_widget(title, main_chunks[0]);

    // --- Tabs ---
    let tab_titles = vec![" Dashboard ", " Interceptor Logs "];
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(color_secondary)))
        .select(state.active_tab)
        .style(Style::default().fg(color_muted))
        .highlight_style(Style::default().fg(color_text).bg(Color::Magenta).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, main_chunks[1]);

    // --- Main View (Tab Routing) ---
    match state.active_tab {
        0 => draw_dashboard(f, &state, main_chunks[2], color_primary, color_secondary, color_success, color_error, color_warning, color_text, color_muted),
        1 => draw_logs(f, &state, list_state, main_chunks[2], color_secondary),
        _ => {}
    }

    // --- Footer ---
    let footer_text = Line::from(vec![
        Span::styled(" [Q] ", Style::default().fg(color_error).add_modifier(Modifier::BOLD)),
        Span::raw("Quit  "),
        Span::styled(" [Tab/Arrows] ", Style::default().fg(color_secondary).add_modifier(Modifier::BOLD)),
        Span::raw("Switch View  "),
        Span::styled(" [Up/Down] ", Style::default().fg(color_primary).add_modifier(Modifier::BOLD)),
        Span::raw("Scroll Logs  "),
    ]);
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(color_muted)));
    f.render_widget(footer, main_chunks[3]);
}

fn draw_dashboard(
    f: &mut Frame,
    state: &crate::state::AppState,
    area: Rect,
    c_primary: Color,
    c_secondary: Color,
    c_success: Color,
    c_error: Color,
    c_warning: Color,
    c_text: Color,
    c_muted: Color,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Stats (Gauge)
            Constraint::Min(8),    // Middle Pane (Tables)
            Constraint::Length(6), // Bottom Mini-Log
        ])
        .split(area);

    // --- Top Stats (Gauge) ---
    let total = state.total_requests;
    let retries = state.total_retries;
    let ratio = if total == 0 { 1.0 } else { (total.saturating_sub(retries)) as f64 / total as f64 };
    let percent = (ratio * 100.0) as u16;

    let gauge_color = if percent > 90 { c_success } else if percent > 70 { c_warning } else { c_error };
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Overall Request Success Rate ").border_style(Style::default().fg(c_primary)))
        .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
        .percent(percent.min(100))
        .label(format!("{}% ({} Requests / {} Retries)", percent, total, retries));
    f.render_widget(gauge, chunks[0]);

    // --- Middle Pane (Tables) ---
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    let active_pool = if state.proxy_pools.is_empty() {
        "None".to_string()
    } else {
        state.proxy_pools[state.active_pool_index].name.clone()
    };

    let pool_rows = vec![
        Row::new(vec![Cell::from("Active Profile"), Cell::from(active_pool).style(Style::default().fg(c_primary).add_modifier(Modifier::BOLD))]),
        Row::new(vec![Cell::from("Available Pools"), Cell::from(state.proxy_pools.len().to_string()).style(Style::default().fg(c_text))]),
    ];
    let pool_table = Table::new(pool_rows, [Constraint::Percentage(40), Constraint::Percentage(60)])
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Proxy Routing ").border_style(Style::default().fg(c_secondary)))
        .column_spacing(1);
    f.render_widget(pool_table, middle_chunks[0]);

    // Tor Table
    let mut tor_rows = Vec::new();
    let mut tor_ports: Vec<&u16> = state.warp_instances.keys().collect();
    tor_ports.sort();

    for port in tor_ports {
        let instance = state.warp_instances.get(port).unwrap();
        let status_color = if instance.status.contains("Running") { c_success } else if instance.status.contains("Bootstrapping") || instance.status.contains("Starting") { c_warning } else { c_error };
        
        let ip_display = instance.ip.as_deref().unwrap_or("Waiting...");
        let ip_color = if instance.ip.is_some() { c_success } else { c_muted };

        tor_rows.push(Row::new(vec![
            Cell::from(format!("Tor-{}", port)).style(Style::default().fg(c_secondary).add_modifier(Modifier::BOLD)),
            Cell::from(instance.status.clone()).style(Style::default().fg(status_color)),
            Cell::from(ip_display).style(Style::default().fg(ip_color)),
        ]));
    }

    if tor_rows.is_empty() {
        tor_rows.push(Row::new(vec![Cell::from("No Tor instances"), Cell::from("-"), Cell::from("-")]));
    }

    let tor_table = Table::new(tor_rows, [Constraint::Length(10), Constraint::Length(25), Constraint::Min(15)])
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Active Tor Engines ").border_style(Style::default().fg(c_success)))
        .header(Row::new(vec!["Instance", "Status", "Public IP"]).style(Style::default().fg(c_text).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)))
        .column_spacing(1);
    f.render_widget(tor_table, middle_chunks[1]);

    // --- Bottom Mini-Log ---
    let log_count = (chunks[2].height.saturating_sub(2)) as usize;
    let recent_logs: Vec<ListItem> = state.logs.iter().rev().take(log_count).rev().map(|msg| {
        let style = if msg.contains("FAIL") || msg.contains("error") {
            Style::default().fg(c_error)
        } else if msg.contains("OK") || msg.contains("successfully") || msg.contains("ready") {
            Style::default().fg(c_success)
        } else if msg.contains("DEBUG") {
            Style::default().fg(c_muted)
        } else {
            Style::default().fg(c_text)
        };
        ListItem::new(Span::styled(msg, style))
    }).collect();

    let mini_logs = List::new(recent_logs)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Live Activity ").border_style(Style::default().fg(c_muted)));
    f.render_widget(mini_logs, chunks[2]);
}

fn draw_logs(f: &mut Frame, state: &crate::state::AppState, list_state: &mut ratatui::widgets::ListState, area: Rect, border_color: Color) {
    let logs: Vec<ListItem> = state.logs.iter().map(|msg| {
        let style = if msg.contains("FAIL") || msg.contains("error") {
            Style::default().fg(Color::LightRed)
        } else if msg.contains("OK") || msg.contains("successfully") || msg.contains("ready") {
            Style::default().fg(Color::LightGreen)
        } else if msg.contains("DEBUG") {
            Style::default().fg(Color::DarkGray)
        } else if msg.contains("Tor-") {
            Style::default().fg(Color::LightMagenta)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(Span::styled(msg, style))
    }).collect();

    let logs_list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Detailed Interceptor Logs ").border_style(Style::default().fg(border_color)))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
        
    f.render_stateful_widget(logs_list, area, list_state);
}
