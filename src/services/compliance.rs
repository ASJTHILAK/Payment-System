use rusqlite::OptionalExtension;
use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::db::{connection::DbConnection, DbError, DbResult};

/// Represents the status of a compliance check
#[derive(Debug, Clone, PartialEq)]
pub enum ComplianceStatus {
    Approved,
    Rejected,
    PendingReview,
}

impl std::fmt::Display for ComplianceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceStatus::Approved => write!(f, "APPROVED"),
            ComplianceStatus::Rejected => write!(f, "REJECTED"),
            ComplianceStatus::PendingReview => write!(f, "PENDING_REVIEW"),
        }
    }
}

impl std::str::FromStr for ComplianceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "APPROVED" => Ok(ComplianceStatus::Approved),
            "REJECTED" => Ok(ComplianceStatus::Rejected),
            "PENDING_REVIEW" => Ok(ComplianceStatus::PendingReview),
            _ => Err(format!("Invalid compliance status: {}", s)),
        }
    }
}

/// Represents a compliance check result
#[derive(Debug)]
#[allow(dead_code)] // Fields are used for API responses and serialization
pub struct ComplianceCheck {
    pub id: String,
    pub transaction_id: String,
    pub source_country: String,
    pub destination_country: String,
    pub amount: f64,
    pub currency: String,
    pub risk_score: f64,
    pub status: ComplianceStatus,
    pub details: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

/// Service for handling compliance checks
#[derive(Clone)]
pub struct ComplianceService {
    db_conn: Arc<DbConnection>,
}

impl ComplianceService {
    pub fn new(db_conn: Arc<DbConnection>) -> Self {
        Self { db_conn }
    }

    /// Run compliance checks using an existing database connection
    pub fn check_compliance_with_conn(
        &self,
        conn: &rusqlite::Connection,
        transaction_id: &str,
        source_country: &str,
        destination_country: &str,
        amount: f64,
        currency: &str,
    ) -> DbResult<ComplianceCheck> {
        debug!(
            "Running compliance check with existing connection: transaction={}, from={}, to={}, amount={}, currency={}",
            transaction_id, source_country, destination_country, amount, currency
        );

        // Calculate risk score based on amount, countries, etc.
        let risk_score = self.calculate_risk_score(source_country, destination_country, amount);

        // Determine compliance status based on risk score
        let (status, details) = if risk_score > 0.8 {
            (
                ComplianceStatus::Rejected,
                Some("Transaction exceeds risk threshold".to_string()),
            )
        } else if risk_score > 0.5 {
            (
                ComplianceStatus::PendingReview,
                Some("Transaction requires manual review".to_string()),
            )
        } else {
            (ComplianceStatus::Approved, None)
        };

        // Save compliance check to database using existing connection
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().naive_utc();
        let status_str = status.to_string();

        conn.execute(
            "INSERT INTO compliance_checks (
                id, transaction_id, source_country, destination_country,
                amount, currency, risk_score, status, details,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                id,
                transaction_id,
                source_country,
                destination_country,
                amount,
                currency,
                risk_score,
                status_str,
                details,
                now,
                now
            ],
        )
        .map_err(|e| {
            error!("Database error saving compliance check: {}", e);
            DbError::from(e.to_string())
        })?;

        info!(
            "Compliance check completed: id={}, transaction={}, status={}, risk_score={}",
            id, transaction_id, status, risk_score
        );

        Ok(ComplianceCheck {
            id,
            transaction_id: transaction_id.to_string(),
            source_country: source_country.to_string(),
            destination_country: destination_country.to_string(),
            amount,
            currency: currency.to_string(),
            risk_score,
            status,
            details,
            created_at: now,
            updated_at: now,
        })
    }

    /// Calculate a risk score for the transaction
    fn calculate_risk_score(
        &self,
        source_country: &str,
        destination_country: &str,
        amount: f64,
    ) -> f64 {
        // This is a simplified risk calculation
        // In a real-world app, this would be much more sophisticated
        let mut risk: f64 = 0.0;

        // Check if countries are high-risk
        let high_risk_countries = vec!["NK", "IR", "CU", "SY", "VE"];
        if high_risk_countries.contains(&source_country)
            || high_risk_countries.contains(&destination_country)
        {
            risk += 0.5;
        }

        // Check for different countries (cross-border)
        if source_country != destination_country {
            risk += 0.1;
        }

        // Check amount thresholds
        if amount > 10_000.0 {
            risk += 0.3;
        } else if amount > 1_000.0 {
            risk += 0.1;
        }

        // Cap risk at 1.0
        risk.min(1.0)
    }

    /// Get compliance check by transaction ID
    pub fn get_compliance_check_by_transaction(
        &self,
        transaction_id: &str,
    ) -> DbResult<Option<ComplianceCheck>> {
        debug!(
            "Fetching compliance check for transaction: {}",
            transaction_id
        );

        let conn = self.db_conn.get();
        let conn = conn.lock().map_err(|e| {
            error!("Failed to acquire database lock: {}", e);
            DbError::from("Failed to acquire database lock")
        })?;

        let result = conn
            .query_row(
                "SELECT id, transaction_id, source_country, destination_country,
                    amount, currency, risk_score, status, details,
                    created_at, updated_at
             FROM compliance_checks
             WHERE transaction_id = ?1",
                rusqlite::params![transaction_id],
                |row| {
                    let status_str: String = row.get(7)?;
                    let status = status_str.parse::<ComplianceStatus>().map_err(|_e| {
                        rusqlite::Error::InvalidColumnType(
                            7,
                            "Invalid status".to_string(),
                            rusqlite::types::Type::Text,
                        )
                    })?;

                    Ok(ComplianceCheck {
                        id: row.get(0)?,
                        transaction_id: row.get(1)?,
                        source_country: row.get(2)?,
                        destination_country: row.get(3)?,
                        amount: row.get(4)?,
                        currency: row.get(5)?,
                        risk_score: row.get(6)?,
                        status,
                        details: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(|e| {
                error!(
                    "Database error fetching compliance check for transaction {}: {}",
                    transaction_id, e
                );
                DbError::from(e.to_string())
            })?;

        Ok(result)
    }
}
