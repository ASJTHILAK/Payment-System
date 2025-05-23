// src/services/compliance.rs
use sqlx::SqlitePool;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::db::{DbError, DbResult};

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
    pool: SqlitePool,
}

impl ComplianceService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run compliance checks for a cross-border transaction
    pub async fn check_compliance(
        &self,
        transaction_id: &str,
        source_country: &str,
        destination_country: &str,
        amount: f64,
        currency: &str,
    ) -> DbResult<ComplianceCheck> {
        debug!(
            "Running compliance check: transaction={}, from={}, to={}, amount={}, currency={}",
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

        // Save compliance check to database
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().naive_utc();
        let status_str = status.to_string();

        sqlx::query!(
            r#"
            INSERT INTO compliance_checks (
                id, transaction_id, source_country, destination_country,
                amount, currency, risk_score, status, details,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
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
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Database error saving compliance check: {}", e);
            DbError::from(e)
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
    pub async fn get_compliance_check_by_transaction(
        &self,
        transaction_id: &str,
    ) -> DbResult<Option<ComplianceCheck>> {
        debug!(
            "Fetching compliance check for transaction: {}",
            transaction_id
        );

        let result = sqlx::query!(
            r#"
            SELECT 
                id as "id!", 
                transaction_id as "transaction_id!", 
                source_country as "source_country!", 
                destination_country as "destination_country!",
                amount as "amount!", 
                currency as "currency!", 
                risk_score as "risk_score!", 
                status as "status!", 
                details,
                created_at as "created_at!", 
                updated_at as "updated_at!"
            FROM compliance_checks
            WHERE transaction_id = ?
            "#,
            transaction_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(
                "Database error fetching compliance check for transaction {}: {}",
                transaction_id, e
            );
            DbError::from(e)
        })?;

        if let Some(row) = result {
            let status = row.status.parse().map_err(|e: String| DbError::from(e))?;

            let check = ComplianceCheck {
                id: row.id,
                transaction_id: row.transaction_id,
                source_country: row.source_country,
                destination_country: row.destination_country,
                amount: row.amount,
                currency: row.currency,
                risk_score: row.risk_score,
                status,
                details: row.details,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };

            debug!(
                "Compliance check found: id={}, status={}, risk_score={}",
                check.id, check.status, check.risk_score
            );

            Ok(Some(check))
        } else {
            debug!(
                "No compliance check found for transaction: {}",
                transaction_id
            );
            Ok(None)
        }
    }
}
