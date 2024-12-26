pub mod db;

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SYMBOLS: &[&str] = &["btcusdt", "ethusdt", "bnbusdt", "xrpusdt"];

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

pub struct OrderBook {
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    pub last_update: Instant,
    pub persistent_orders: HashMap<String, OrderBookEntry>,
}

pub struct App {
    pub order_books: HashMap<String, OrderBook>,
    pub current_symbol: String,
    pub message_history: Vec<OrderBookMessage>,
    pub db: db::Database,
    pub last_db_write: Instant,
    analysis_buffer: HashMap<String, Vec<(Instant, usize, usize)>>, // (timestamp, total_orders, human_orders) per symbol
}

pub struct MarketAnalysis {
    pub total_orders: usize,
    pub likely_human_orders: usize,
    pub bot_patterns: Vec<String>,
    pub human_patterns: Vec<String>,
    pub confidence_scores: HashMap<String, f64>,
}

impl App {
    pub fn new() -> Result<App, Box<dyn std::error::Error>> {
        let db = db::Database::new()?;

        Ok(App {
            order_books: crate::SYMBOLS
                .iter()
                .map(|&symbol| {
                    (
                        symbol.to_uppercase(),
                        OrderBook {
                            bids: Vec::new(),
                            asks: Vec::new(),
                            last_update: Instant::now(),
                            persistent_orders: HashMap::new(),
                        },
                    )
                })
                .collect(),
            current_symbol: "BTCUSDT".to_string(),
            message_history: Vec::with_capacity(10000),
            db,
            last_db_write: Instant::now(),
            analysis_buffer: HashMap::new(),
        })
    }

    fn update_analysis_buffer(&mut self, symbol: &str, total_orders: usize, human_orders: usize) {
        let now = Instant::now();
        let buffer = self.analysis_buffer.entry(symbol.to_string()).or_default();

        // Add new data point
        buffer.push((now, total_orders, human_orders));

        // Remove data points older than 5 seconds
        buffer.retain(|(timestamp, _, _)| timestamp.elapsed() < Duration::from_secs(5));
    }

    fn calculate_average_analysis(&self, symbol: &str) -> Option<(f64, f64)> {
        if let Some(buffer) = self.analysis_buffer.get(symbol) {
            if buffer.is_empty() {
                return None;
            }

            let total_sum: usize = buffer.iter().map(|(_, total, _)| total).sum();
            let human_sum: usize = buffer.iter().map(|(_, _, human)| human).sum();
            let count = buffer.len();

            Some((
                total_sum as f64 / count as f64,
                human_sum as f64 / count as f64,
            ))
        } else {
            None
        }
    }

    pub fn analyze_market(&mut self) -> MarketAnalysis {
        let current_symbol = self.current_symbol.clone();
        let analysis = if let Some(order_book) = self.order_books.get(&current_symbol) {
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

            let total_orders = order_book.bids.len() + order_book.asks.len();

            // Update the analysis buffer
            self.update_analysis_buffer(&current_symbol, total_orders, likely_human_orders);

            // Write to database every 5 seconds using averaged data
            if self.last_db_write.elapsed() >= Duration::from_secs(5) {
                if let Some((avg_total, avg_human)) =
                    self.calculate_average_analysis(&current_symbol)
                {
                    let record = db::MarketAnalysisRecord {
                        symbol: current_symbol.clone(),
                        timestamp: db::get_current_timestamp(),
                        total_orders: avg_total as i64,
                        human_orders: avg_human as i64,
                        bot_orders: (avg_total - avg_human) as i64,
                        human_ratio: if avg_total > 0.0 {
                            avg_human / avg_total
                        } else {
                            0.0
                        },
                    };

                    if let Err(e) = self.db.insert_analysis(&record) {
                        eprintln!("Failed to store market analysis: {}", e);
                    }
                    self.last_db_write = Instant::now();
                }
            }

            MarketAnalysis {
                total_orders,
                likely_human_orders,
                bot_patterns,
                human_patterns,
                confidence_scores,
            }
        } else {
            MarketAnalysis::default()
        };

        analysis
    }

    fn analyze_round_numbers(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        if let Some(order_book) = self.order_books.get(&self.current_symbol) {
            for order in order_book.bids.iter().chain(order_book.asks.iter()) {
                if let Ok(price) = order.price.parse::<f64>() {
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
                        let diff = (price2 - price1).abs();
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

    pub fn next_symbol(&mut self) {
        let symbols: Vec<_> = self.order_books.keys().cloned().collect();
        if let Some(pos) = symbols.iter().position(|s| s == &self.current_symbol) {
            self.current_symbol = symbols[(pos + 1) % symbols.len()].clone();
        }
    }

    pub fn update_orders(&mut self, result: &serde_json::Value) {
        if let Some(symbol) = result.get("symbol").and_then(|s| s.as_str()) {
            if let Some(order_book) = self.order_books.get_mut(symbol) {
                // Clear existing orders
                order_book.bids.clear();
                order_book.asks.clear();

                // Process bids
                if let Some(bids) = result.get("bids").and_then(|b| b.as_array()) {
                    for bid in bids {
                        if let (Some(price), Some(quantity)) = (bid[0].as_str(), bid[1].as_str()) {
                            let total = price.parse::<f64>().unwrap_or(0.0)
                                * quantity.parse::<f64>().unwrap_or(0.0);
                            let entry = OrderBookEntry {
                                price: price.to_string(),
                                quantity: quantity.to_string(),
                                total,
                                is_likely_human: false, // Will be updated by analysis
                                human_indicators: Vec::new(),
                            };
                            order_book.bids.push(entry);
                        }
                    }
                }

                // Process asks
                if let Some(asks) = result.get("asks").and_then(|a| a.as_array()) {
                    for ask in asks {
                        if let (Some(price), Some(quantity)) = (ask[0].as_str(), ask[1].as_str()) {
                            let total = price.parse::<f64>().unwrap_or(0.0)
                                * quantity.parse::<f64>().unwrap_or(0.0);
                            let entry = OrderBookEntry {
                                price: price.to_string(),
                                quantity: quantity.to_string(),
                                total,
                                is_likely_human: false, // Will be updated by analysis
                                human_indicators: Vec::new(),
                            };
                            order_book.asks.push(entry);
                        }
                    }
                }

                // Sort bids in descending order (highest price first)
                order_book.bids.sort_by(|a, b| {
                    b.price
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        .partial_cmp(&a.price.parse::<f64>().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Sort asks in ascending order (lowest price first)
                order_book.asks.sort_by(|a, b| {
                    a.price
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        .partial_cmp(&b.price.parse::<f64>().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Update last update time
                order_book.last_update = std::time::Instant::now();

                // Add to message history
                let side = if !order_book.bids.is_empty() {
                    OrderSide::Bid
                } else {
                    OrderSide::Ask
                };

                let entry = if !order_book.bids.is_empty() {
                    &order_book.bids[0]
                } else if !order_book.asks.is_empty() {
                    &order_book.asks[0]
                } else {
                    return;
                };

                let message = OrderBookMessage {
                    timestamp: std::time::Instant::now(),
                    symbol: symbol.to_string(),
                    is_human: entry.is_likely_human,
                    price: entry.price.clone(),
                    quantity: entry.quantity.clone(),
                    side,
                };

                self.message_history.push(message);

                // Keep message history size reasonable
                if self.message_history.len() > 10000 {
                    self.message_history.drain(0..5000);
                }
            }
        }
    }
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
