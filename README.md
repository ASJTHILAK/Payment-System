# Payment System

A modern, secure RESTful API for digital payments built with Rust and Axum. Supports domestic and cross-border payments with built-in compliance checks and real-time currency conversion.

## Features

- **User Authentication**: Secure JWT-based authentication system with token refresh capabilities
- **Account Management**: Create and manage user payment accounts with multi-currency and multi-country support
- **Transaction Processing**: Process secure domestic and cross-border money transfers with automatic currency conversion
- **Currency Exchange**: Real-time currency conversion with rate caching and historical rate tracking
- **Cross-Border Compliance**: Automated risk assessment and compliance checks for international transfers
- **Validation**: Comprehensive input validation for all endpoints with detailed error messages
- **Error Handling**: Detailed error responses with proper HTTP status codes for client applications
- **Database**: SQLite persistence with SQLx for type-safe queries and automatic migrations
- **Rate Limiting**: IP-based rate limiting to prevent abuse and ensure service stability
- **Audit Trail**: Detailed transaction history with original and converted amounts for cross-border payments

## Tech Stack

- **Rust**: Memory-safe systems programming language
- **Axum**: High-performance web framework built on top of Tokio
- **SQLx**: Async SQL database driver with compile-time query checking
- **SQLite**: Lightweight embedded database
- **JWT**: JSON Web Tokens for secure authentication
- **Bcrypt**: Industry-standard password hashing
- **Docker**: Containerization for easy deployment

## API Endpoints

### Authentication

- `POST /api/auth/register` - Register a new user
- `POST /api/auth/login` - Authenticate user and receive JWT tokens
- `POST /api/auth/refresh` - Refresh access token
- `POST /api/auth/logout` - Invalidate current tokens

### Transactions (Protected Routes)

- `POST /api/protected/transactions/create` - Create a new transaction
  - Supports both domestic and cross-border payments
  - Automatic currency conversion with `convert_currency: true`
- `GET /api/protected/transactions/list` - List user's transactions
- `GET /api/protected/transactions/compliance/:transaction_id` - Get compliance information for a cross-border transaction
- `GET /api/protected/transactions/exchange-rates/:currency` - Get current exchange rates for a base currency

### Users (Protected Routes)

- `GET /api/protected/users/me` - Get current user info
- `GET /api/protected/users/account` - Get user's account details

## Getting Started

### Prerequisites

- Rust 1.65+ and Cargo
- SQLite 3.39+
- SQLx CLI (`cargo install sqlx-cli`)

### Environment Variables

Create a `.env` file in the project root:

```env
DATABASE_URL="sqlite:data.db"
JWT_SECRET="asjthilak"
PORT=3000
GLOBAL_RATE_LIMIT=300
AUTH_RATE_LIMIT=30
EXCHANGE_RATE_API_KEY="your_api_key"
```

- `DATABASE_URL`: SQLite database connection string
- `JWT_SECRET`: Secret key for JWT token signing
- `PORT`: Server port (default: 3000)
- `GLOBAL_RATE_LIMIT`: Maximum requests allowed per minute globally (default: 300)
- `AUTH_RATE_LIMIT`: Maximum requests allowed per minute for authentication endpoints (default: 30)
- `EXCHANGE_RATE_API_KEY`: Sign in to [ExchangeRate-API](https://app.exchangerate-api.com/) to get API key which is required for cross-border payments

### Running Locally

```bash
# Install dependencies
cargo install sqlx-cli

# Create database and run migrations
cargo sqlx database create
cargo sqlx migrate run

# Build the project
cargo build

# Start the server
cargo run
```

### Docker Deployment

```bash
# Build the Docker image
docker build -t payment-system:latest .

# Run the container
docker run -p 3000:3000 payment-system:latest
```

## Development

### Running Tests

```bash
cargo test
```

### API Documentation

The API uses standard HTTP status codes and JSON responses. All protected endpoints require a valid JWT token in the Authorization header:

```
Authorization: Bearer your_jwt_token
```

Complete API definitions are available in OpenAPI/Swagger format in the `api-docs.yml` file. You can use tools like [Swagger UI](https://swagger.io/tools/swagger-ui/) or [Redoc](https://redocly.github.io/redoc/) to visualize and interact with the API documentation.

### Cross-Border Payments

#### Supported Currencies
The system supports the following currencies for transactions:
- INR (Indian Rupee)
- USD (US Dollar)
- EUR (Euro)
- GBP (British Pound)
- SGD (Singapore Dollar)
- AED (UAE Dirham)
- Others can be added through configuration

#### Compliance Rules
Cross-border transactions undergo automated compliance checks based on:
- Amount thresholds (extra checks for amounts > 1,000 or > 10,000)
- Country risk levels (some countries require additional verification)
- High-risk jurisdictions: NK (North Korea), IR (Iran), CU (Cuba), SY (Syria), VE (Venezuela)
- Risk scores range from 0.0 to 1.0:
  - 0.0-0.5: Approved automatically
  - 0.5-0.8: Requires manual review
  - >0.8: Automatically rejected

Exchange rates are cached for 6 hours to ensure consistent rates during transaction processing while maintaining reasonable accuracy.

## Project Structure

```
payment-system/
├── src/
│   ├── db/           # Database operations and schema
│   ├── handlers/     # API endpoint handlers
│   ├── middleware/   # Auth middleware and JWT validation
│   ├── models/       # Data models and validation
│   ├── services/     # Business logic services
│   ├── utils/        # Utility functions and helpers
│   ├── error.rs      # Error definitions
│   ├── lib.rs        # Library exports
│   └── main.rs       # Application entry point
├── migrations/       # SQLx database migrations
├── tests/            # Integration tests
└── Dockerfile        # Container configuration
```

## License

This project is licensed under the MIT License.