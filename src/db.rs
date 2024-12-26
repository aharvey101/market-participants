use rusqlite::{Connection, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MarketAnalysisRecord {
    pub symbol: String,
    pub timestamp: u64,
    pub total_orders: i64,
    pub human_orders: i64,
    pub bot_orders: i64,
    pub human_ratio: f64,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let conn = Connection::open("market_analysis.db")?;

        // Create the table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS market_analysis (
                id INTEGER PRIMARY KEY,
                symbol TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                total_orders INTEGER NOT NULL,
                human_orders INTEGER NOT NULL,
                bot_orders INTEGER NOT NULL,
                human_ratio REAL NOT NULL
            )",
            [],
        )?;

        Ok(Database { conn })
    }

    pub fn insert_analysis(&self, record: &MarketAnalysisRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO market_analysis (
                symbol, timestamp, total_orders, human_orders, bot_orders, human_ratio
            ) VALUES (?, ?, ?, ?, ?, ?)",
            (
                &record.symbol,
                record.timestamp,
                record.total_orders,
                record.human_orders,
                record.bot_orders,
                record.human_ratio,
            ),
        )?;
        Ok(())
    }

    pub fn get_latest_analysis(&self, symbol: &str) -> Result<Option<MarketAnalysisRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT symbol, timestamp, total_orders, human_orders, bot_orders, human_ratio 
             FROM market_analysis 
             WHERE symbol = ? 
             ORDER BY timestamp DESC 
             LIMIT 1",
        )?;

        let mut rows = stmt.query([symbol])?;

        if let Some(row) = rows.next()? {
            Ok(Some(MarketAnalysisRecord {
                symbol: row.get(0)?,
                timestamp: row.get(1)?,
                total_orders: row.get(2)?,
                human_orders: row.get(3)?,
                bot_orders: row.get(4)?,
                human_ratio: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_analysis_history(
        &self,
        symbol: &str,
        limit: i64,
    ) -> Result<Vec<MarketAnalysisRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT symbol, timestamp, total_orders, human_orders, bot_orders, human_ratio 
             FROM market_analysis 
             WHERE symbol = ? 
             ORDER BY timestamp DESC 
             LIMIT ?",
        )?;

        let rows = stmt.query_map([symbol, &limit.to_string()], |row| {
            Ok(MarketAnalysisRecord {
                symbol: row.get(0)?,
                timestamp: row.get(1)?,
                total_orders: row.get(2)?,
                human_orders: row.get(3)?,
                bot_orders: row.get(4)?,
                human_ratio: row.get(5)?,
            })
        })?;

        let mut records = Vec::new();
        for record in rows {
            records.push(record?);
        }
        Ok(records)
    }
}

pub fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
