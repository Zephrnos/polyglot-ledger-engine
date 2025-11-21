use crate::models::transaction::Transaction;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres}; 
use tokio::join;

async fn get_account_balance(pool: &PgPool, account_id: i32) -> Result<Decimal, sqlx::Error> {
    let balance: Decimal = sqlx::query_scalar::<_, Decimal>("SELECT balance FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    Ok(balance)
}

pub async fn verify(pool: &PgPool, transaction: &Transaction) -> Result<(), String> {
    if transaction.value() <= Decimal::ZERO {
        return Err("Transaction value must be positive".to_string());
    }
    if transaction.source() == transaction.target() {
        return Err("Target and Source same".to_string());
    }

    let (source_result, target_result) = join!(
        get_account_balance(pool, transaction.source()),
        get_account_balance(pool, transaction.target())
    );

    match (source_result, target_result) {
        (Ok(source_value), Ok(_target_value)) => {
            if source_value < transaction.value() {
                Err("Source account has insufficient funds".to_string())
            } else {
                Ok(())
            }
        }
        (Err(_), Ok(_)) => Err("Source account not found".to_string()),
        (Ok(_), Err(_)) => Err("Target account not found".to_string()),
        (Err(_), Err(_)) => Err("Both accounts not found".to_string()),
    }
}

// Accepts &mut Transaction for atomicity
async fn push_transaction(tx: &mut sqlx::Transaction<'_, Postgres>, transaction: &Transaction) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE accounts SET balance = balance - $1 WHERE id = $2")
        .bind(transaction.value())
        .bind(transaction.source())
        .execute(&mut **tx) 
        .await?;

    sqlx::query("UPDATE accounts SET balance = balance + $1 WHERE id = $2")
        .bind(transaction.value())
        .bind(transaction.target())
        .execute(&mut **tx)
        .await?;
        
    Ok(())
}

pub async fn transact(pool: &PgPool, transaction: Transaction) -> Result<(), String> {
    // 1. Verify (Read Phase)
    verify(pool, &transaction).await?;

    // 2. Start Atomic Transaction
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // 3. Attempt Updates
    push_transaction(&mut tx, &transaction).await.map_err(|e| e.to_string())?;

    // 4. Commit
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(())
}

// --- 5. Updated Test Module for SQLx 0.8 ---
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transaction::Transaction;
    use rust_decimal_macros::dec;
    use sqlx::PgPool;

    async fn setup_schema(pool: &PgPool) {
        sqlx::query("CREATE TABLE IF NOT EXISTS accounts (id INT PRIMARY KEY, balance DECIMAL)")
            .execute(pool)
            .await
            .expect("Failed to create schema");
    }

    #[sqlx::test]
    async fn test_verify_insufficient_funds(pool: PgPool) {
        setup_schema(&pool).await;
        
        sqlx::query("INSERT INTO accounts (id, balance) VALUES ($1, $2), ($3, $4)")
            .bind(1).bind(dec!(50.00))
            .bind(2).bind(dec!(200.00))
            .execute(&pool)
            .await
            .unwrap();

        let transaction = Transaction::new(
            1, 2, dec!(100.00)
        );

        let res = verify(&pool, &transaction).await;
        assert_eq!(res.unwrap_err(), "Source account has insufficient funds");
    }

    #[sqlx::test]
    async fn test_verify_source_account_not_found(pool: PgPool) {
        setup_schema(&pool).await;
        
        sqlx::query("INSERT INTO accounts (id, balance) VALUES ($1, $2)")
            .bind(2).bind(dec!(200.00))
            .execute(&pool)
            .await
            .unwrap();

        let transaction = Transaction::new(
             99, 2, dec!(100.00)
        );

        let res = verify(&pool, &transaction).await;
        assert_eq!(res.unwrap_err(), "Source account not found");
    }

    #[sqlx::test]
    async fn test_successful_transaction(pool: PgPool) {
        setup_schema(&pool).await;

        sqlx::query("INSERT INTO accounts (id, balance) VALUES ($1, $2), ($3, $4)")
            .bind(1).bind(dec!(100.00))
            .bind(2).bind(dec!(50.00))
            .execute(&pool)
            .await
            .unwrap();
        
        let transaction = Transaction::new(
            1, 2, dec!(25.00)
        );

        let res = transact(&pool, transaction).await;

        assert!(res.is_ok());

        let new_source_bal: Decimal = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        let new_target_bal: Decimal = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = 2")
            .fetch_one(&pool).await.unwrap();

        assert_eq!(new_source_bal, dec!(75.00));
        assert_eq!(new_target_bal, dec!(75.00));
    }
}