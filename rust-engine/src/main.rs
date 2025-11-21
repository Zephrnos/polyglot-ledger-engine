use clap::Parser;
use futures_lite::stream::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueDeclareOptions},
    types::FieldTable,
    Connection, ConnectionProperties,
};
use redis::AsyncCommands; // For updating status
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use std::error::Error;
use uuid::Uuid;
use chrono::Utc;

mod core; 
mod models;

use crate::models::transaction::Transaction;
use crate::core::worker::transact;

#[derive(Debug, Deserialize)]
struct TransferRequestDto {
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

    #[arg(short, long, default_value = "postgres://postgres:password@localhost:5432/postgres")]
    db_url: String,

    #[arg(long, default_value = "redis://localhost:6379/")]
    redis_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    println!("🚀 Worker Starting...");

    // 1. Connect to Postgres
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.db_url)
        .await?;
    println!("✅ Connected to Postgres");

    // 2. Connect to Redis
    let redis_client = redis::Client::open(args.redis_url.clone())?;
    // We use multiplexed connection which is standard for redis-rs 0.24+
    let mut redis_conn = redis_client.get_multiplexed_async_connection().await?;
    println!("✅ Connected to Redis at {}", args.redis_url);

    // 3. Connect to RabbitMQ
    let conn = Connection::connect(&args.amqp_addr, ConnectionProperties::default()).await?;
    let channel = conn.create_channel().await?;
    println!("✅ Connected to RabbitMQ");

    // 4. Declare Queue
    let _queue = channel
        .queue_declare(
            "transactions",
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await?;

    // 5. Create Consumer
    let mut consumer = channel
        .basic_consume(
            "transactions",
            "rust_worker_debug", // Specific tag for this worker
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    println!("🎧 Waiting for transactions...");

    while let Some(delivery) = consumer.next().await {
        if let Ok(delivery) = delivery {
            let req: TransferRequestDto = match serde_json::from_slice(&delivery.data) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("❌ Malformed JSON: {}", e);
                    delivery.nack(BasicNackOptions { requeue: false, ..Default::default() }).await?;
                    continue;
                }
            };

            println!("---------------------------------------------------");
            println!("📥 Processing Job [{}]", req.idempotency_key);

            let transaction = Transaction::new(
                Uuid::new_v4(),
                Utc::now(),
                req.source_id,
                req.target_id,
                req.amount
            );

            let redis_key = format!("status:{}", req.idempotency_key);
            
            match transact(&pool, transaction).await {
                Ok(_) => {
                    println!("💰 Database Transaction Committed.");
                    
                    println!("📝 Attempting to write 'success' to Redis key: {}", redis_key);
                    
                    // Explicitly handling Redis errors (No more silent failures)
                    match redis_conn.set::<_, _, ()>(&redis_key, "success").await {
                        Ok(_) => println!("✅ Redis Update Successful"),
                        Err(e) => println!("❌ REDIS WRITE FAILED: {}", e),
                    }
                    
                    delivery.ack(BasicAckOptions::default()).await?;
                }
                Err(e) => {
                    eprintln!("⚠️ Transaction Logic Failed: {}", e);
                    
                    println!("📝 Writing failure reason to Redis...");
                    match redis_conn.set::<_, _, ()>(&redis_key, format!("failed: {}", e)).await {
                        Ok(_) => println!("✅ Redis Update Successful"),
                        Err(e) => println!("❌ REDIS WRITE FAILED: {}", e),
                    }
                    
                    delivery.ack(BasicAckOptions::default()).await?;
                }
            }
        }
    }

    Ok(())
}