use binance_ws::{App, OrderBookMessage, OrderSide};
use ratatui::{
    prelude::*,
    symbols,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
};
use std::time::Duration;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Title
            Constraint::Percentage(70), // Graph
            Constraint::Length(10),     // Stats
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(format!(
        "Market Analysis - {} (Press 'q' to quit, 'n' for next symbol)",
        app.current_symbol
    ))
    .style(Style::default().fg(Color::White));
    f.render_widget(title, chunks[0]);

    // Get historical data for the current symbol
    let mut history = app
        .db
        .get_analysis_history(&app.current_symbol, 100)
        .unwrap_or_default();

    // Reverse history so oldest is first
    history.reverse();

    // Find the earliest timestamp to use as reference point
    let start_time = history.first().map(|r| r.timestamp).unwrap_or_default();

    // Prepare data for the graph
    let human_data: Vec<(f64, f64)> = history
        .iter()
        .map(|record| {
            let x = (record.timestamp - start_time) as f64;
            let percentage = record.human_ratio * 100.0;
            (x, percentage)
        })
        .collect();

    let bot_data: Vec<(f64, f64)> = history
        .iter()
        .map(|record| {
            let x = (record.timestamp - start_time) as f64;
            let percentage = (1.0 - record.human_ratio) * 100.0;
            (x, percentage)
        })
        .collect();

    // Calculate time range
    let time_range = if let (Some(first), Some(last)) = (history.first(), history.last()) {
        (last.timestamp - first.timestamp) as f64
    } else {
        60.0 // Default to 60 seconds if no data
    };

    // Create datasets
    let datasets = vec![
        Dataset::default()
            .name("Human Traders")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Green))
            .data(&human_data),
        Dataset::default()
            .name("Bot Traders")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Red))
            .data(&bot_data),
    ];

    // Create the chart
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title("Trading Activity")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time")
                .bounds([0.0, time_range])
                .labels(vec![
                    Span::raw("0s"),
                    Span::raw(format!("{}s", time_range as u64)),
                ]),
        )
        .y_axis(
            Axis::default()
                .title("Percentage")
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("50%"), Span::raw("100%")]),
        );

    f.render_widget(chart, chunks[1]);

    // Current stats
    let analysis = app.analyze_market();
    let stats_text = vec![
        format!("Current Statistics for {}:", app.current_symbol),
        format!("Total Orders: {}", analysis.total_orders),
        format!("Human Orders: {}", analysis.likely_human_orders),
        format!(
            "Current Human Ratio: {:.1}%",
            if analysis.total_orders > 0 {
                (analysis.likely_human_orders as f64 / analysis.total_orders as f64) * 100.0
            } else {
                0.0
            }
        ),
        format!("Data Points: {}", history.len()),
    ]
    .join("\n");

    let stats = Paragraph::new(stats_text)
        .block(
            Block::default()
                .title("Current Stats")
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::Yellow));

    f.render_widget(stats, chunks[2]);
}
