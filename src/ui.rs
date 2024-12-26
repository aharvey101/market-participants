use crate::SYMBOLS;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Row, Table},
};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct OrderBookEntry {
    pub price: String,
    pub quantity: String,
    pub total: f64,
    pub is_likely_human: bool,
    pub human_indicators: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OrderBookMessage {
    pub timestamp: Instant,
    pub symbol: String,
    pub is_human: bool,
    pub price: String,
    pub quantity: String,
    pub side: OrderSide,
}

#[derive(Debug, Clone)]
pub enum OrderSide {
    Bid,
    Ask,
}

pub struct App {
    pub order_books: HashMap<String, OrderBook>,
    pub current_symbol: String,
    pub message_history: Vec<OrderBookMessage>,
}

pub struct OrderBook {
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    pub last_update: Instant,
}

pub struct MarketAnalysis {
    pub total_orders: usize,
    pub likely_human_orders: usize,
    pub bot_patterns: Vec<String>,
    pub human_patterns: Vec<String>,
    pub confidence_scores: HashMap<String, f64>,
}

impl Default for MarketAnalysis {
    fn default() -> Self {
        MarketAnalysis {
            total_orders: 0,
            likely_human_orders: 0,
            bot_patterns: Vec::new(),
            human_patterns: Vec::new(),
            confidence_scores: HashMap::new(),
        }
    }
}

impl App {
    pub fn new() -> App {
        App {
            order_books: SYMBOLS
                .iter()
                .map(|&symbol| {
                    (
                        symbol.to_uppercase(),
                        OrderBook {
                            bids: Vec::new(),
                            asks: Vec::new(),
                            last_update: Instant::now(),
                        },
                    )
                })
                .collect(),
            current_symbol: "BTCUSDT".to_string(),
            message_history: Vec::with_capacity(10000),
        }
    }

    pub fn add_to_history(&mut self, entry: OrderBookEntry, side: OrderSide) {
        let message = OrderBookMessage {
            timestamp: Instant::now(),
            symbol: self.current_symbol.clone(),
            is_human: entry.is_likely_human,
            price: entry.price,
            quantity: entry.quantity,
            side,
        };

        self.message_history.push(message);
        if self.message_history.len() > 10000 {
            self.message_history.remove(0);
        }
    }

    fn update_history_for_orders(
        &mut self,
        symbol: &str,
        bids: Vec<OrderBookEntry>,
        asks: Vec<OrderBookEntry>,
    ) {
        for bid in bids {
            self.add_to_history(bid, OrderSide::Bid);
        }
        for ask in asks {
            self.add_to_history(ask, OrderSide::Ask);
        }
    }

    pub fn update_orders(&mut self, result: &serde_json::Value) {
        if let Some(symbol) = result.get("symbol").and_then(Value::as_str) {
            let symbol_upper = symbol.to_uppercase();
            let current_symbol = self.current_symbol.clone();
            let analysis = if symbol_upper == current_symbol {
                self.analyze_market()
            } else {
                MarketAnalysis::default()
            };

            if let Some(order_book) = self.order_books.get_mut(&symbol_upper) {
                order_book.bids.clear();
                order_book.asks.clear();

                if let Some(bids) = result.get("bids").and_then(|b| b.as_array()) {
                    for bid in bids {
                        let price = bid[0].as_str().unwrap_or("0").to_string();
                        let quantity = bid[1].as_str().unwrap_or("0").to_string();
                        let price_f = price.parse::<f64>().unwrap_or(0.0);
                        let quantity_f = quantity.parse::<f64>().unwrap_or(0.0);
                        order_book.bids.push(OrderBookEntry {
                            price,
                            quantity,
                            total: price_f * quantity_f,
                            is_likely_human: false,
                            human_indicators: Vec::new(),
                        });
                    }
                }

                if let Some(asks) = result.get("asks").and_then(|a| a.as_array()) {
                    for ask in asks {
                        let price = ask[0].as_str().unwrap_or("0").to_string();
                        let quantity = ask[1].as_str().unwrap_or("0").to_string();
                        let price_f = price.parse::<f64>().unwrap_or(0.0);
                        let quantity_f = quantity.parse::<f64>().unwrap_or(0.0);
                        order_book.asks.push(OrderBookEntry {
                            price,
                            quantity,
                            total: price_f * quantity_f,
                            is_likely_human: false,
                            human_indicators: Vec::new(),
                        });
                    }
                }

                // Use the pre-computed scores
                for bid in &mut order_book.bids {
                    if let Some(&score) = analysis.confidence_scores.get(&bid.price) {
                        bid.is_likely_human = score > 0.6;
                        if bid.is_likely_human {
                            bid.human_indicators
                                .push(format!("Confidence: {:.1}%", score * 100.0));
                        }
                    }
                }

                for ask in &mut order_book.asks {
                    if let Some(&score) = analysis.confidence_scores.get(&ask.price) {
                        ask.is_likely_human = score > 0.6;
                        if ask.is_likely_human {
                            ask.human_indicators
                                .push(format!("Confidence: {:.1}%", score * 100.0));
                        }
                    }
                }

                order_book.last_update = Instant::now();

                // Clone orders before releasing borrow
                let bids = order_book.bids.clone();
                let asks = order_book.asks.clone();

                // Update history separately
                self.update_history_for_orders(&symbol_upper, bids, asks);
            }
        }
    }

    fn analyze_round_numbers(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();

        if let Some(order_book) = self.order_books.get(&self.current_symbol) {
            for order in order_book.bids.iter().chain(order_book.asks.iter()) {
                if let Ok(price) = order.price.parse::<f64>() {
                    // Check for psychological price levels
                    let decimal_part = price.fract();
                    let whole_part = price.trunc();

                    let is_round =
                        decimal_part == 0.0 || decimal_part == 0.5 || decimal_part == 0.25;

                    let is_psychological = whole_part % 1000.0 == 0.0 || // e.g., 50000
                        whole_part % 500.0 == 0.0 ||  // e.g., 49500
                        whole_part % 100.0 == 0.0; // e.g., 49100

                    results.push((order.price.clone(), is_round || is_psychological));
                }
            }
        }
        results
    }

    fn analyze_order_sizes(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();

        if let Some(order_book) = self.order_books.get(&self.current_symbol) {
            for order in order_book.bids.iter().chain(order_book.asks.iter()) {
                if let Ok(quantity) = order.quantity.parse::<f64>() {
                    let whole_part = quantity.trunc();
                    let decimal_part = quantity.fract();

                    // Check for human-like quantities
                    let is_human_like = decimal_part == 0.0 ||  // Whole numbers
                        decimal_part == 0.5 ||  // Half units
                        decimal_part == 0.25 || // Quarter units
                        whole_part <= 10.0 ||   // Small round numbers
                        whole_part % 5.0 == 0.0; // Multiples of 5

                    results.push((order.quantity.clone(), is_human_like));
                }
            }
        }
        results
    }

    fn analyze_order_placement(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();

        if let Some(order_book) = self.order_books.get(&self.current_symbol) {
            for orders in [&order_book.bids, &order_book.asks] {
                for window in orders.windows(2) {
                    if let (Ok(price1), Ok(price2)) = (
                        window[0].price.parse::<f64>(),
                        window[1].price.parse::<f64>(),
                    ) {
                        // Calculate price difference
                        let diff = (price2 - price1).abs();

                        // Bots often place orders at very precise intervals
                        // Humans tend to be more "messy"
                        let is_human_like = diff > 0.01 && // Not too precise
                            diff.fract() != 0.0 && // Not perfectly spaced
                            diff % 0.1 != 0.0; // Not aligned to common intervals

                        results.push((window[0].price.clone(), is_human_like));
                    }
                }
            }
        }
        results
    }

    pub fn analyze_market(&self) -> MarketAnalysis {
        if let Some(order_book) = self.order_books.get(&self.current_symbol) {
            let round_numbers = self.analyze_round_numbers();
            let order_sizes = self.analyze_order_sizes();
            let order_placement = self.analyze_order_placement();

            let mut confidence_scores = HashMap::new();
            let mut human_patterns = Vec::new();
            let mut bot_patterns = Vec::new();

            // Combine analyses
            for (price, indicators) in round_numbers
                .iter()
                .zip(order_sizes.iter())
                .zip(order_placement.iter())
                .map(|((a, b), c)| (a.0.clone(), vec![a.1, b.1, c.1]))
            {
                let human_score =
                    indicators.iter().filter(|&&x| x).count() as f64 / indicators.len() as f64;

                confidence_scores.insert(price.clone(), human_score);

                if human_score > 0.6 {
                    human_patterns.push(format!("Order at {} shows human behavior", price));
                } else {
                    bot_patterns.push(format!("Order at {} likely automated", price));
                }
            }

            let likely_human_orders = confidence_scores
                .values()
                .filter(|&&score| score > 0.6)
                .count();

            MarketAnalysis {
                total_orders: order_book.bids.len() + order_book.asks.len(),
                likely_human_orders,
                bot_patterns,
                human_patterns,
                confidence_scores,
            }
        } else {
            MarketAnalysis {
                total_orders: 0,
                likely_human_orders: 0,
                bot_patterns: Vec::new(),
                human_patterns: Vec::new(),
                confidence_scores: HashMap::new(),
            }
        }
    }

    pub fn next_symbol(&mut self) {
        let symbols: Vec<_> = self.order_books.keys().cloned().collect();
        if let Some(pos) = symbols.iter().position(|s| s == &self.current_symbol) {
            self.current_symbol = symbols[(pos + 1) % symbols.len()].clone();
        }
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Title
            Constraint::Percentage(50), // Order book
            Constraint::Length(10),     // Analysis
            Constraint::Min(0),         // History
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(format!(
        "Order Book - {} (Press 'q' to quit, 'n' for next symbol)",
        app.current_symbol
    ))
    .style(Style::default().fg(Color::White));
    f.render_widget(title, chunks[0]);

    // Order book layout
    let order_book_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[1]);

    // Define the column widths once
    let column_widths = [
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(34),
    ];

    if let Some(order_book) = app.order_books.get(&app.current_symbol) {
        // Bids (green)
        let bids: Vec<Row> = order_book
            .bids
            .iter()
            .map(|bid| {
                let style = if bid.is_likely_human {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::Green)
                };

                let indicators = if bid.is_likely_human {
                    "👤 " // Human indicator
                } else {
                    "🤖 " // Bot indicator
                };

                Row::new(vec![
                    format!("{}{}", indicators, bid.price),
                    bid.quantity.clone(),
                    format!("{:.2}", bid.total),
                ])
                .style(style)
            })
            .collect();

        let bids_table = Table::new(bids, column_widths)
            .header(Row::new(vec!["Price", "Quantity", "Total"]))
            .block(Block::default().title("Bids").borders(Borders::ALL));

        // Asks (red)
        let asks: Vec<Row> = order_book
            .asks
            .iter()
            .map(|ask| {
                let style = if ask.is_likely_human {
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::Red)
                };

                let indicators = if ask.is_likely_human {
                    "👤 " // Human indicator
                } else {
                    "🤖 " // Bot indicator
                };

                Row::new(vec![
                    format!("{}{}", indicators, ask.price),
                    ask.quantity.clone(),
                    format!("{:.2}", ask.total),
                ])
                .style(style)
            })
            .collect();

        let asks_table = Table::new(asks, column_widths)
            .header(Row::new(vec!["Price", "Quantity", "Total"]))
            .block(Block::default().title("Asks").borders(Borders::ALL));

        f.render_widget(bids_table, order_book_chunks[0]);
        f.render_widget(asks_table, order_book_chunks[1]);
    }

    // Add analysis section
    let analysis = app.analyze_market();
    let analysis_text = vec![
        format!("Total Orders: {}", analysis.total_orders),
        format!("Likely Human Orders: {}", analysis.likely_human_orders),
        format!(
            "Human Ratio: {:.1}%",
            (analysis.likely_human_orders as f64 / analysis.total_orders as f64) * 100.0
        ),
        "".to_string(),
        "Legend:".to_string(),
        "👤 Human Order   🤖 Bot Order".to_string(),
        "".to_string(),
        "Recent Human Activity:".to_string(),
    ]
    .into_iter()
    .chain(
        analysis
            .human_patterns
            .iter()
            .take(3)
            .map(|p| format!("👤 {}", p)),
    )
    .collect::<Vec<String>>()
    .join("\n");

    let analysis_widget = Paragraph::new(analysis_text)
        .block(
            Block::default()
                .title("Market Analysis")
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::Yellow));

    f.render_widget(analysis_widget, chunks[2]);

    // Add history section
    let history_messages: Vec<Row> = app
        .message_history
        .iter()
        .rev() // Show newest first
        .take(100) // Show last 100 messages
        .map(|msg| {
            let style = match (msg.is_human, &msg.side) {
                (true, OrderSide::Bid) => Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
                (true, OrderSide::Ask) => {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                }
                (false, OrderSide::Bid) => Style::default().fg(Color::Green),
                (false, OrderSide::Ask) => Style::default().fg(Color::Red),
            };

            let elapsed = msg.timestamp.elapsed();
            let time_str = format!(
                "{:02}:{:02}:{:02}",
                elapsed.as_secs() / 3600,
                (elapsed.as_secs() % 3600) / 60,
                elapsed.as_secs() % 60
            );

            Row::new(vec![
                time_str,
                msg.symbol.clone(),
                if msg.is_human { "👤" } else { "🤖" }.to_string(),
                match msg.side {
                    OrderSide::Bid => "BID",
                    OrderSide::Ask => "ASK",
                }
                .to_string(),
                msg.price.clone(),
                msg.quantity.clone(),
            ])
            .style(style)
        })
        .collect();

    let history_table = Table::new(
        history_messages,
        [
            Constraint::Length(8),  // Time
            Constraint::Length(8),  // Symbol
            Constraint::Length(2),  // Human/Bot
            Constraint::Length(4),  // Side
            Constraint::Length(10), // Price
            Constraint::Length(10), // Quantity
        ],
    )
    .header(Row::new(vec![
        "Time", "Symbol", "Type", "Side", "Price", "Quantity",
    ]))
    .block(
        Block::default()
            .title("Message History")
            .borders(Borders::ALL),
    );

    f.render_widget(history_table, chunks[3]);
}
