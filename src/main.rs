mod ui;

use binance_ws::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use serde_json::{json, Value};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const SYMBOLS: &[&str] = &["btcusdt", "ethusdt", "bnbusdt", "xrpusdt"];
const UPDATE_SPEED: &str = "100ms"; // Options: 100ms, 1000ms
const DEPTH_LEVELS: u32 = 20; // Options: 5, 10, 20

#[derive(Debug)]
struct WebSocketState {
    last_update: Instant,
    reconnect_attempts: u32,
    snapshot_received: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for communication
    let (tx, mut rx) = mpsc::channel(32);
    let tx_clone = tx.clone();

    // Spawn WebSocket handler
    tokio::spawn(async move {
        if let Err(e) = run_websocket(tx_clone).await {
            eprintln!("WebSocket error: {}", e);
        }
    });

    // Create app state
    let mut app = match App::new() {
        Ok(app) => app,
        Err(e) => {
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            return Err(format!("Failed to initialize application: {}", e).into());
        }
    };

    loop {
        // Check for user input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('n') => app.next_symbol(),
                    _ => {}
                }
            }
        }

        // Check for new order book updates
        while let Ok(result) = rx.try_recv() {
            app.update_orders(&result);
        }

        // Draw UI
        terminal.draw(|f| ui::draw(f, &mut app))?;
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

async fn run_websocket(tx: mpsc::Sender<Value>) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = WebSocketState {
        last_update: Instant::now(),
        reconnect_attempts: 0,
        snapshot_received: false,
    };

    loop {
        match connect_and_stream(&tx, &mut state).await {
            Ok(_) => {
                // Successful completion (probably disconnect)
                state.reconnect_attempts = 0;
                println!("WebSocket disconnected, attempting to reconnect...");
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                state.reconnect_attempts += 1;
            }
        }

        // Exponential backoff for reconnection
        let delay = RECONNECT_DELAY.mul_f64(1.5f64.powi(state.reconnect_attempts as i32));
        sleep(delay).await;
    }
}

async fn connect_and_stream(
    tx: &mpsc::Sender<Value>,
    state: &mut WebSocketState,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create combined stream for multiple symbols - using regular WebSocket stream
    let streams: Vec<String> = SYMBOLS
        .iter()
        .map(|&symbol| format!("{}@depth@{}", symbol, UPDATE_SPEED))
        .collect();

    // Use the regular WebSocket stream URL
    let url = Url::parse(&format!(
        "wss://stream.binance.com:9443/stream?streams={}",
        streams.join("/")
    ))?;

    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&url).await?;
    let (_write, mut read) = ws_stream.split();

    // Get initial snapshots for all symbols
    for &symbol in SYMBOLS {
        let snapshot = fetch_initial_snapshot(symbol).await?;
        tx.send(snapshot).await?;
    }
    state.snapshot_received = true;

    // Process stream messages
    while let Some(msg) = read.next().await {
        state.last_update = Instant::now();

        match msg? {
            Message::Text(text) => {
                let response: Value = serde_json::from_str(&text)?;

                if let Some(data) = response.get("data") {
                    let transformed = json!({
                        "symbol": data["s"].as_str().unwrap_or("UNKNOWN").to_uppercase(),
                        "bids": data["b"],
                        "asks": data["a"],
                        "lastUpdateId": data["u"]
                    });
                    tx.send(transformed).await?;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }

        // Check for stale connection (no updates for 10 seconds)
        if state.last_update.elapsed() > Duration::from_secs(10) {
            return Err("Connection stale".into());
        }
    }

    Ok(())
}

async fn fetch_initial_snapshot(symbol: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.binance.com/api/v3/depth?symbol={}&limit={}",
        symbol.to_uppercase(),
        DEPTH_LEVELS
    );

    let response = reqwest::get(&url).await?.json::<Value>().await?;
    Ok(json!({
        "symbol": symbol.to_uppercase(),
        "bids": response["bids"],
        "asks": response["asks"],
        "lastUpdateId": response["lastUpdateId"]
    }))
}
