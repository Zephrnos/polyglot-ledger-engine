use crate::models::Transaction;

fn query_value(account_id: i32) -> Result<Decimal, ()> {
    todo() // Pull account details from a SQL Server
}

pub fn verify(transaction: &Transaction) -> Result<(), String> {

    if transaction.value <= Decimal::ZERO {
        return Err("Transaction value must be positive".to_string());
    }

    if transaction.source == transaction.target {
        return Err("Target and Source same".to_string());
    }

    match (query_value(transaction.source), query_value(transaction.target)) {
        
        (Ok(source_value), Ok(_target_value)) => {
            if source_value < transaction.value {
                Err("Source account has insufficient funds".to_string())
            } else {
                Ok(())
            }
        },
        (Err(_), Ok(_)) => Err("Source not found".to_string()),
        (Ok(_), Err(_)) => Err("Target not found".to_string()),
        (Err(_), Err(_)) => Err("Accounts DNE".to_string())
    }
}

fn push_transaction(transaction: Transaction) -> Result<(), String> {
    todo(); // Push the transaction details to a SQL Server
}

pub fn transact(transaction: Transaction) -> Result<(), String> {

    verify(&transaction)?;

    push_transaction(transaction)?;

    Ok(())

}