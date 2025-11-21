use clap::Parser;
use futures_lite::stream::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueDeclareOptions},
    types::FieldTable,
    Connection, ConnectionProperties,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use std::error::Error;
use uuid::Uuid;
use chrono::Utc;

mod core;
mod models;
// Import your existing logic
use crate::models::transaction::Transaction;
use crate::core::worker::transact;

// --- DTO: Matches the JSON sent by Go ---
#[derive(Debug, Deserialize)]
struct TransferRequestDto {
    // Go sends: "idempotency_key", "source_id", etc.
    // Rust defaults to snake_case, so these match automatically.
    idempotency_key: String,
    source_id: i32,
    target_id: i32,
    amount: Decimal,
}

#[derive(Parser, Debug)]
#[command(about = "Worker process that listens to RabbitMQ.")]
struct Args {
    #[arg(short, long, default_value = "amqp://guest:guest@localhost:5672/%2f")]
    amqp_addr: String,

    #[arg(short, long, default_value = "postgres://user:password@localhost/db_name")]
    db_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // 1. Connect to Database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.db_url)
        .await?;
    println!("✅ Connected to Postgres");

    // 2. Connect to RabbitMQ
    let conn = Connection::connect(&args.amqp_addr, ConnectionProperties::default()).await?;
    let channel = conn.create_channel().await?;
    println!("✅ Connected to RabbitMQ");

    // 3. Declare Queue (Must match Go configuration)
    let _queue = channel
        .queue_declare(
            "transactions", // Same name as in Go
            QueueDeclareOptions {
                durable: true, // Matches Go's "DurableQueueConfig"
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await?;

    // 4. Create Consumer
    let mut consumer = channel
        .basic_consume(
            "transactions",
            "rust_worker_1", // Consumer tag
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    println!("🎧 Waiting for transactions...");

    // 5. Processing Loop
    while let Some(delivery) = consumer.next().await {
        if let Ok(delivery) = delivery {
            // A. Parse JSON
            // We use serde_json to convert bytes -> Rust Struct
            let req: TransferRequestDto = match serde_json::from_slice(&delivery.data) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("❌ Malformed JSON: {}", e);
                    // Nack without requeue (poison message)
                    delivery.nack(BasicNackOptions { requeue: false, ..Default::default() }).await?;
                    continue;
                }
            };

            println!("📥 Received Job: Transfer {} from {} to {}", req.amount, req.source_id, req.target_id);

            // B. Convert DTO to Domain Model
            // Your Transaction::new takes a UUID. Since Go sent a string key, 
            // we generate a new internal UUID or parse the key if it's a UUID.
            // For now, we generate a new tracking ID for the ledger.
            let transaction = Transaction::new(
                Uuid::new_v4(), 
                Utc::now(),
                req.source_id, 
                req.target_id, 
                req.amount
            );

            // C. Execute Logic (The "Rust Core" box in diagram)
            match transact(&pool, transaction).await {
                Ok(_) => {
                    // --- DIAGRAM STEP: Acknowledge Message ---
                    println!("✅ Transaction Success!");
                    delivery.ack(BasicAckOptions::default()).await?;
                }
                Err(e) => {
                    // --- DIAGRAM STEP: Failure Handling ---
                    eprintln!("⚠️ Transaction Failed: {}", e);
                    
                    // Note: In a real system, you might want to "Publish Failure Event" here 
                    // as per the diagram. For now, we just Log and Nack.
                    // requeue: false -> Send to Dead Letter Queue (if configured) or discard.
                    delivery.nack(BasicNackOptions { requeue: false, ..Default::default() }).await?;
                }
            }
        }
    }

    Ok(())
}