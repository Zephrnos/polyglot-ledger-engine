use rust_decimal::Decimal;

#[derive(Debug, sqlx::FromRow)]
struct Account {
    account_id: i32,    // SERIAL (int4) maps to i32
    value: Decimal,     // This type is precise and safe for money
}

impl Account {

    pub fn new(account_id: i32, value: Decimal) -> Self {
        Account {
            account_id,
            value
        }
    }

    pub fn account_id(&self) -> i32 {
        self.account_id
    }

    pub fn value(&self) -> Decimal {
        self.value
    }

}