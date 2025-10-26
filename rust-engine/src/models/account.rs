use rust_decimal::Decimal;

#[derive(Debug, sqlx::FromRow)]
struct Account {
    account_id: i32,   // SERIAL (int4) maps to i32
    value: Decimal, // This type is precise and safe for money
}