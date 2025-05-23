-- Migration script for Payment System

-- Create users table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create accounts table with country support for cross-border payments
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    balance REAL NOT NULL DEFAULT 0.0,
    currency TEXT NOT NULL DEFAULT 'INR',
    country TEXT DEFAULT 'IN',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (id) REFERENCES users(id)
);

-- Create transactions table with cross-border payment support
CREATE TABLE IF NOT EXISTS transactions (
    id TEXT PRIMARY KEY,
    from_account_id TEXT NOT NULL,
    to_account_id TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    description TEXT,
    exchange_rate REAL DEFAULT NULL,
    original_amount REAL DEFAULT NULL,
    original_currency TEXT DEFAULT NULL,
    is_cross_border BOOLEAN DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (from_account_id) REFERENCES accounts(id),
    FOREIGN KEY (to_account_id) REFERENCES accounts(id)
);

-- Create exchange rates table to cache currency rates
CREATE TABLE IF NOT EXISTS exchange_rates (
    base_currency TEXT NOT NULL,
    target_currency TEXT NOT NULL,
    rate REAL NOT NULL,
    last_updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (base_currency, target_currency)
);

-- Create compliance checks table for cross-border transactions
CREATE TABLE IF NOT EXISTS compliance_checks (
    id TEXT PRIMARY KEY,
    transaction_id TEXT NOT NULL,
    source_country TEXT NOT NULL,
    destination_country TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT NOT NULL,
    risk_score REAL NOT NULL,
    status TEXT NOT NULL,
    details TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (transaction_id) REFERENCES transactions(id)
);

-- Create indices for improved query performance
CREATE INDEX idx_transactions_from_account ON transactions(from_account_id);
CREATE INDEX idx_transactions_to_account ON transactions(to_account_id);
CREATE INDEX idx_compliance_transaction ON compliance_checks(transaction_id);
CREATE INDEX idx_exchange_rates ON exchange_rates(base_currency, target_currency);
