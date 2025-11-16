use crate::models::Transaction;
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tokio::join;

// --- 1. Your query function, now async ---
//
// * It's now 'async fn'
// * It takes the 'pool' to execute the query
// * It returns a Result with sqlx::Error
async fn get_account_balance(
    pool: &PgPool,
    account_id: i32,
) -> Result<Decimal, sqlx::Error> {
    
    // We use query_scalar to get a single value (scalar)
    // We also tell it what type to expect: <Decimal>
    let balance = sqlx::query_scalar!(
        Decimal, // The type we expect to get back
        "SELECT balance FROM accounts WHERE id = $1", // Your SQL query
        account_id
    )
    .fetch_one(pool) // Execute and get the single row
    .await?; // Await the database call

    Ok(balance)
}

pub async fn verify(
    pool: &PgPool,
    transaction: &Transaction,
) -> Result<(), String> {
    if transaction.value <= Decimal::ZERO {
        return Err("Transaction value must be positive".to_string());
    }

    if transaction.source == transaction.target {
        return Err("Target and Source same".to_string());
    }

    // Use tokio::join! to run both queries at the same time
    let (source_result, target_result) = join!(
        get_account_balance(pool, transaction.source),
        get_account_balance(pool, transaction.target)
    );

    // The match logic is now matching on the Results of the async calls
    match (source_result, target_result) {
        (Ok(source_value), Ok(_target_value)) => {
            if source_value < transaction.value {
                Err("Source account has insufficient funds".to_string())
            } else {
                Ok(())
            }
        }
        // These arms will catch any sqlx::Error (like RowNotFound)
        (Err(_), Ok(_)) => Err("Source account not found".to_string()),
        (Ok(_), Err(_)) => Err("Target account not found".to_string()),
        (Err(_), Err(_)) => Err("Both accounts not found".to_string()),
    }
}

// --- 3. Your push_transaction function, now async ---
async fn push_transaction(
    pool: &PgPool,
    transaction: &Transaction,
) -> Result<(), sqlx::Error> {
    // A real transaction would be better, but for this example:
    sqlx::query!(
        "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
        transaction.value,
        transaction.source
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
        transaction.value,
        transaction.target
    )
    .execute(pool)
    .await?;

    Ok(())
}

// --- 4. Your top-level transact function, now async ---
pub async fn transact(
    pool: &PgPool,
    transaction: Transaction,
) -> Result<(), String> {
    
    // We must .await the async verify function
    verify(pool, &transaction).await?;

    // We must .await the async push function
    // Map the sqlx::Error to a String error
    push_transaction(pool, &transaction)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*; // Import your functions (verify, transact, etc.)
    use rust_decimal_macros::dec;
    use sqlx::mock::Mock; // The main mock component
    use sqlx::postgres::{PgConnectOptions, PgPool}; // Use your specific DB
    use sqlx::{ConnectOptions, Error as SqlxError};
    use std::str::FromStr;

    // --- HELPER FUNCTION ---
    // Creates the mock pool for your tests.
    // This is just boilerplate you can copy.
    async fn create_mock_pool() -> PgPool {
        // We parse a "mock:" connection string.
        let opts = PgConnectOptions::from_str("mock:")
            .expect("Failed to create mock options");
        
        // This pool "looks" like a PgPool but is a mock.
        PgPool::connect_with(opts).await.unwrap()
    }

    #[tokio::test] // Standard tokio test
    async fn test_verify_insufficient_funds_mocked() {
        // 1. SETUP: Create the mock pool and transaction
        let pool = create_mock_pool().await;
        let transaction = Transaction {
            source: 1,
            target: 2,
            value: dec!(100.00), // Trying to send 100
        };

        // 2. DEFINE EXPECTATIONS:
        
        // Expect the query for the SOURCE account (id 1)
        Mock::given(
            // We match the exact query created by query_scalar!
            sqlx::query_scalar!(
                Decimal,
                "SELECT balance FROM accounts WHERE id = $1",
                transaction.source // = 1
            )
        )
        .expect(1) // Expect it to be called 1 time
        .respond_with(
            // Respond with a mock row containing one column
            sqlx::mock::MockRow::new().append(dec!(50.00)) // Source only has 50!
        );

        // Expect the query for the TARGET account (id 2)
        Mock::given(
            sqlx::query_scalar!(
                Decimal,
                "SELECT balance FROM accounts WHERE id = $1",
                transaction.target // = 2
            )
        )
        .expect(1) // Expect it 1 time
        .respond_with(
            // Respond with a mock row
            sqlx::mock::MockRow::new().append(dec!(200.00))
        );

        // 3. ACTION: Call the function
        let res = verify(&pool, &transaction).await;

        // 4. ASSERT:
        assert_eq!(
            res.unwrap_err(),
            "Source account has insufficient funds"
        );
    }

    #[tokio::test]
    async fn test_verify_source_account_not_found_mocked() {
        // 1. SETUP
        let pool = create_mock_pool().await;
        let transaction = Transaction {
            source: 99, // Non-existent account
            target: 2,
            value: dec!(100.00),
        };

        // 2. DEFINE EXPECTATIONS:
        
        // Expect the query for the SOURCE account (id 99)
        Mock::given(
            sqlx::query_scalar!(
                Decimal,
                "SELECT balance FROM accounts WHERE id = $1",
                transaction.source // = 99
            )
        )
        .expect(1)
        .respond_with(
            // This time, respond with a database error!
            // This simulates what happens when fetch_one() finds 0 rows.
            Err(SqlxError::RowNotFound)
        );

        // NOTE: We don't need to mock the target query,
        // because our code will short-circuit and return an Err
        // after the first query fails.

        // 3. ACTION
        let res = verify(&pool, &transaction).await;

        // 4. ASSERT
        assert_eq!(res.unwrap_err(), "Source account not found");
    }

    #[tokio::test]
    async fn test_successful_transaction_mocked() {
        // 1. SETUP
        let pool = create_mock_pool().await;
        let transaction = Transaction {
            source: 1,
            target: 2,
            value: dec!(25.00),
        };
        // Clone it so we can pass it to transact()
        let tx_clone = transaction.clone(); 

        // 2. DEFINE EXPECTATIONS for transact()
        
        // --- First, the 'verify' part will run ---
        Mock::given(
            sqlx::query_scalar!(
                Decimal, "SELECT balance FROM accounts WHERE id = $1",
                transaction.source // = 1
            )
        )
        .expect(1)
        .respond_with(sqlx::mock::MockRow::new().append(dec!(100.00))); // Has 100

        Mock::given(
            sqlx::query_scalar!(
                Decimal, "SELECT balance FROM accounts WHERE id = $1",
                transaction.target // = 2
            )
        )
        .expect(1)
        .respond_with(sqlx::mock::MockRow::new().append(dec!(50.00))); // Has 50

        // --- Second, the 'push_transaction' part will run ---
        // Expect the UPDATE on the source account
        Mock::given(
            sqlx::query!(
                "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
                transaction.value, // = 25.00
                transaction.source // = 1
            )
        )
        .expect(1)
        .respond_with(sqlx::mock::MockExec::new().with_rows_affected(1));

        // Expect the UPDATE on the target account
        Mock::given(
            sqlx::query!(
                "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
                transaction.value, // = 25.00
                transaction.target // = 2
            )
        )
        .expect(1)
        .respond_with(sqlx::mock::MockExec::new().with_rows_affected(1));

        // 3. ACTION
        let res = transact(&pool, tx_clone).await;

        // 4. ASSERT
        assert!(res.is_ok());
    }
}