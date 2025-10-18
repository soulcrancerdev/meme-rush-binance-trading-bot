use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::time::sleep;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, NewMac};
use sha2::Sha256;
use base64::{encode, decode};
use colored::*;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

// Binance API credentials (replace with your own)
const BINANCE_API_KEY: &str = "<YOUR_BINANCE_API_KEY>";
const BINANCE_API_SECRET: &str = "<YOUR_BINANCE_API_SECRET>";

// Binance API base URL
const BINANCE_API_URL: &str = "https://api.binance.com";

// Data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Token {
    symbol: String,
    name: String,
    address: String,
    id: String,
    liquidity_bnb: f64,
    binance_listed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Balance {
    free: f64,
    locked: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountInfo {
    balances: Vec<BalanceItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BalanceItem {
    asset: String,
    free: String,
    locked: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MarketPrice {
    symbol: String,
    price: String,
}

// Utility functions for Binance API signing
fn binance_signature(query_string: &str, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(query_string.as_bytes());
    let result = mac.finalize();
    let signature_bytes = result.into_bytes();
    hex::encode(signature_bytes)
}

// Helper to build signed query string
fn build_signed_query(params: &HashMap<&str, String>, secret: &str) -> String {
    let query_string: String = params.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("&");
    let signature = binance_signature(&query_string, secret);
    format!("{}&signature={}", query_string, signature)
}

// Fetch account balances
async fn get_account_info(client: &Client) -> Option<AccountInfo> {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let mut params = HashMap::new();
    params.insert("timestamp", timestamp.clone());
    let query = build_signed_query(&params, BINANCE_API_SECRET);

    let url = format!("{}/api/v3/account?{}", BINANCE_API_URL, query);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(BINANCE_API_KEY).unwrap());

    match client.get(&url).headers(headers).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<AccountInfo>().await {
                    Ok(info) => Some(info),
                    Err(e) => {
                        eprintln!("Error parsing account info: {:?}", e);
                        None
                    }
                }
            } else {
                eprintln!("Error fetching account info: {:?}", resp.status());
                None
            }
        }
        Err(e) => {
            eprintln!("HTTP error: {:?}", e);
            None
        }
    }
}

// Get current price for a symbol
async fn get_symbol_price(client: &Client, symbol: &str) -> Option<f64> {
    let url = format!("{}/api/v3/ticker/price?symbol={}", BINANCE_API_URL, symbol);
    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<MarketPrice>().await {
                    Ok(price_data) => {
                        price_data.price.parse().ok()
                    }
                    Err(e) => {
                        eprintln!("Error parsing price: {:?}", e);
                        None
                    }
                }
            } else {
                eprintln!("Error fetching price: {:?}", resp.status());
                None
            }
        }
        Err(e) => {
            eprintln!("HTTP error: {:?}", e);
            None
        }
    }
}

// Place an order (buy/sell)
async fn place_order(
    client: &Client,
    symbol: &str,
    side: &str, // "BUY" or "SELL"
    quantity: f64,
) -> bool {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let mut params = HashMap::new();
    params.insert("symbol", symbol.to_string());
    params.insert("side", side.to_string());
    params.insert("type", "MARKET".to_string());
    params.insert("quantity", quantity.to_string());
    params.insert("timestamp", timestamp);

    let query = build_signed_query(&params, BINANCE_API_SECRET);
    let url = format!("{}/api/v3/order", BINANCE_API_URL);

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(BINANCE_API_KEY).unwrap());
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));

    match client.post(&url).headers(headers).body(query).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                println!(
                    "{} order placed: {} {}",
                    side.green(),
                    quantity,
                    symbol.yellow()
                );
                true
            } else {
                eprintln!("Order error: {:?}", resp.text().await);
                false
            }
        }
        Err(e) => {
            eprintln!("HTTP error: {:?}", e);
            false
        }
    }
}

// Fetch token price from CoinGecko
async fn get_token_price_coingecko(token_id: &str) -> Option<f64> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
        token_id
    );
    let client = Client::new();
    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let json: serde_json::Value = resp.json().await.unwrap_or_default();
                json[token_id]["usd"].as_f64()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

// Main trading logic
async fn trading_loop(
    client: &Client,
    token: &Token,
    wallet_address: &Option<serde_json::Value>,
) {
    loop {
        // Fetch current price
        let symbol = format!("{}USDT", token.symbol);
        if let Some(price) = get_symbol_price(client, &symbol).await {
            println!("Current price of {}: ${}", symbol, price);

            // Placeholder: simple buy low/sell high logic
            // You can implement a real strategy here
            if price < 0.8 * 1.0 { // example threshold
                println!("Price low, buying...");
                // Fetch wallet balance and decide quantity
                let quantity = 10.0; // example
                place_order(client, &symbol, "BUY", quantity).await;
            } else if price > 1.2 * 1.0 {
                println!("Price high, selling...");
                let quantity = 10.0; // example
                place_order(client, &symbol, "SELL", quantity).await;
            }
        }
        sleep(Duration::from_secs(30)).await; // wait before next check
    }
}

// Entry point
#[tokio::main]
async fn main() {
    println!("{}", "Starting Meme Rush Binance Trading Bot".cyan());

    let client = Client::new();

    // Load tokens
    let meme_tokens = vec![
        Token {
            symbol: "MEME".to_string(),
            name: "Meme Token".to_string(),
            address: "0x123...".to_string(),
            id: "meme-token".to_string(),
            liquidity_bnb: 150.0,
            binance_listed: false,
        }
    ];

    // Fetch wallet info
    if let Some(account_info) = get_account_info(&client).await {
        println!("Account balances fetched.");
        for balance in &account_info.balances {
            println!("{}: {}", balance.asset, balance.free);
        }
    }

    // Run trading for each token
    for token in &meme_tokens {
        tokio::spawn(trading_loop(&client, token, &None)).await.unwrap();
    }

    // Keep alive
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
