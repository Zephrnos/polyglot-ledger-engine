use crate::models::account_data::Account;
use rust_decimal::Decimal;
use uuid::Uuid;
use chrono::{DateTime, Utc};

type AccountId = i32;

pub struct Transaction {
    id: Uuid,
    created_at: DateTime<Utc>,
    source: AccountId,
    target: AccountId,
    value: Decimal
}

pub impl Transaction {

    pub fn verify(&self) -> Result<(), String> {
        
    }

    pub fn transact(&self) -> Result<(), String> {

    }

}