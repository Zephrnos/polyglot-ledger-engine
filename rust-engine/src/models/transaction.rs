use rust_decimal::Decimal;
use uuid::Uuid;
use chrono::{DateTime, Utc};

type AccountId = i32;

pub struct Transaction {
    id: Uuid,
    date: DateTime<Utc>,
    source: AccountId,
    target: AccountId,
    value: Decimal
}

impl Transaction {

    pub fn new(id: Uuid, date: DateTime<Utc>, source: AccountId, target: AccountId, value: Decimal) -> Self {
        Transaction {
            id,
            date,
            source,
            target,
            value
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn date(&self) -> DateTime<Utc> {
        self.date
    }

    pub fn source(&self) -> AccountId {
        self.source
    }

    pub fn target(&self) -> AccountId {
        self.target
    }

    pub fn value(&self) -> Decimal {
        self.value
    }

}