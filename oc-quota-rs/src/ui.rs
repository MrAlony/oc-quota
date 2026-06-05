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
    let color_secondary = Color::Magenta;
    let color_success = Color::Green;
    let color_error = Color::Red;
    let color_warning = Color::Yellow;
    let color_text = Color::White;
    let color_muted = Color::DarkGray;

    let size = f.area();

    // Global Layout: Header (3), Main (flex), Footer (3)
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header & Tabs
            Constraint::Min(10),   // Main View
            Constraint::Length(3), // Footer
        ])
        .split(size);

    // --- Header & Tabs ---
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[0]);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" 🚀 OC-QUOTA ", Style::default().fg(color_primary).add_modifier(Modifier::BOLD)),
        Span::styled("Unified Proxy ", Style::default().fg(color_text)),
        Span::styled("v1.0 ", Style::default().fg(color_muted)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(color_primary)));
    f.render_widget(title, header_chunks[0]);

    let tab_titles = vec![" Dashboard ", " Interceptor Logs "];
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(color_secondary)))
        .select(state.active_tab)
        .style(Style::default().fg(color_muted))
        .highlight_style(Style::default().fg(color_secondary).add_modifier(Modifier::BOLD | Modifier::REVERSED));
    f.render_widget(tabs, header_chunks[1]);

    // --- Main View (Tab Routing) ---
    match state.active_tab {
        0 => draw_dashboard(f, &state, main_chunks[1], color_primary, color_secondary, color_success, color_error, color_warning, color_text, color_muted),
        1 => draw_logs(f, &state, list_state, main_chunks[1], color_secondary),
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
    f.render_widget(footer, main_chunks[2]);
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
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // --- Left Pane: Interceptor Stats ---
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(100)])
        .split(chunks[0]);

    let active_pool = if state.proxy_pools.is_empty() {
        "None".to_string()
    } else {
        state.proxy_pools[state.active_pool_index].name.clone()
    };

    let interceptor_rows = vec![
        Row::new(vec![Cell::from("Total Requests"), Cell::from(state.total_requests.to_string()).style(Style::default().fg(c_text).add_modifier(Modifier::BOLD))]),
        Row::new(vec![Cell::from("Total Retries"), Cell::from(state.total_retries.to_string()).style(Style::default().fg(c_warning))]),
        Row::new(vec![Cell::from("Active Pool"), Cell::from(active_pool).style(Style::default().fg(c_secondary))]),
        Row::new(vec![Cell::from("Pool Size"), Cell::from(state.proxy_pools.len().to_string())]),
    ];
    let interceptor_table = Table::new(interceptor_rows, [Constraint::Percentage(50), Constraint::Percentage(50)])
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Traffic Interceptor ").border_style(Style::default().fg(c_primary)))
        .column_spacing(1);
    f.render_widget(interceptor_table, left_chunks[0]);


    // --- Right Pane: Tor Instances ---
    let mut tor_rows = Vec::new();
    let mut tor_ports: Vec<&u16> = state.warp_instances.keys().collect();
    tor_ports.sort();

    for port in tor_ports {
        let instance = state.warp_instances.get(port).unwrap();
        let color = if instance.status.contains("Running") { c_success } else { c_warning };
        
        let ip_display = instance.ip.as_deref().unwrap_or("Waiting...");
        let ip_color = if instance.ip.is_some() { c_success } else { c_muted };

        tor_rows.push(Row::new(vec![
            Cell::from(format!("Tor-{}", port)).style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from(instance.status.clone()).style(Style::default().fg(color)),
            Cell::from(ip_display).style(Style::default().fg(ip_color)),
        ]));
    }

    if tor_rows.is_empty() {
        tor_rows.push(Row::new(vec![Cell::from("No Tor instances"), Cell::from("-"), Cell::from("-")]));
    }

    let tor_table = Table::new(tor_rows, [Constraint::Length(10), Constraint::Length(22), Constraint::Min(15)])
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Active Tor Rotation Engines ").border_style(Style::default().fg(c_secondary)))
        .header(Row::new(vec!["Instance", "Status", "Public IP"]).style(Style::default().fg(c_text).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)))
        .column_spacing(1);
    f.render_widget(tor_table, chunks[1]);
}

fn draw_logs(f: &mut Frame, state: &crate::state::AppState, list_state: &mut ratatui::widgets::ListState, area: Rect, border_color: Color) {
    let logs: Vec<ListItem> = state.logs.iter().map(|msg| {
        let style = if msg.contains("FAIL") || msg.contains("error") {
            Style::default().fg(Color::Red)
        } else if msg.contains("OK") || msg.contains("successfully") {
            Style::default().fg(Color::Green)
        } else if msg.contains("Tor-") {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(Span::styled(msg, style))
    }).collect();

    let logs_list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Live Interceptor Logs ").border_style(Style::default().fg(border_color)))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
        
    f.render_stateful_widget(logs_list, area, list_state);
}
