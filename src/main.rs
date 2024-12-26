mod ui;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use serde_json::{json, Value};
use std::{io, time::Duration};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for communication between WebSocket and UI
    let (tx, mut rx) = mpsc::channel(32);
    let tx_clone = tx.clone();

    // Spawn WebSocket handler
    tokio::spawn(async move {
        if let Err(e) = run_websocket(tx_clone).await {
            eprintln!("WebSocket error: {}", e);
        }
    });

    // Create app state
    let mut app = ui::App::new();

    loop {
        // Check for user input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Check for new order book updates
        while let Ok(result) = rx.try_recv() {
            app.update_orders(&result);
        }

        // Draw UI
        terminal.draw(|f| ui::draw(f, &app))?;
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
    // Connect to Binance WebSocket stream API (different URL)
    let url = Url::parse("wss://stream.binance.com:9443/ws/btcusdt@depth20@100ms")?;
    let (ws_stream, _) = connect_async(url).await?;
    let (_write, mut read) = ws_stream.split();

    // Remove the interval and request logic since we're now using streams
    loop {
        if let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    let response: Value = serde_json::from_str(&text)?;
                    // Stream format is different, so we need to transform it
                    let transformed = json!({
                        "result": {
                            "bids": response["bids"],
                            "asks": response["asks"]
                        }
                    });
                    if let Some(result) = transformed.get("result") {
                        tx.send(result.clone()).await?;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    }

    Ok(())
}
