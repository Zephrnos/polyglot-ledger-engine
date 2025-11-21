use rust_decimal::Decimal;

type AccountId = i32;

#[derive(Clone)]
pub struct Transaction {
    source: AccountId,
    target: AccountId,
    value: Decimal
}

impl Transaction {

    pub fn new(source: AccountId, target: AccountId, value: Decimal) -> Self {
        Transaction {
            source,
            target,
            value
        }
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