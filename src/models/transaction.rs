use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: String,
    pub from_account_id: String,
    pub to_account_id: String,
    pub amount: f64,
    pub currency: String,
    pub status: TransactionStatus,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Completed,
    Failed,
}

impl std::str::FromStr for TransactionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PENDING" => Ok(TransactionStatus::Pending),
            "COMPLETED" => Ok(TransactionStatus::Completed),
            "FAILED" => Ok(TransactionStatus::Failed),
            _ => Err(format!("Invalid status: {}", s)),
        }
    }
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Pending => write!(f, "PENDING"),
            TransactionStatus::Completed => write!(f, "COMPLETED"),
            TransactionStatus::Failed => write!(f, "FAILED"),
        }
    }
}

// SQLx implementations
impl sqlx::Type<sqlx::Sqlite> for TransactionStatus {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for TransactionStatus {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let str_value = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        str_value.parse::<Self>().map_err(|e: String| e.into())
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for TransactionStatus {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> sqlx::encode::IsNull {
        args.push(sqlx::sqlite::SqliteArgumentValue::Text(
            self.to_string().into(),
        ));
        sqlx::encode::IsNull::No
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateTransactionRequest {
    #[validate(length(equal = 36))] // UUID length
    pub to_account_id: String,
    #[validate(range(min = 0.01))]
    pub amount: f64,
    #[validate(length(equal = 3))] // ISO 4217 currency code length
    pub currency: String,
    #[validate(length(max = 200))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: String, // This is now the same as user_id
    pub balance: f64,
    pub currency: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateAccountRequest {
    #[validate(custom = "validate_currency")]
    pub currency: String,
    #[validate(range(min = 0.0))]
    pub minimum_balance: f64,
}

fn validate_currency(currency: &str) -> Result<(), validator::ValidationError> {
    if currency != "INR" {
        return Err(validator::ValidationError::new("only_inr_allowed"));
    }
    Ok(())
}
