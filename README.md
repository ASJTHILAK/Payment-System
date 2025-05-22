# Payment System

A modern, secure RESTful API for digital payments built with Rust and Axum.

## Features

- **User Authentication**: Secure JWT-based authentication system with token refresh capabilities
- **Account Management**: Create and manage user payment accounts 
- **Transaction Processing**: Perform secure money transfers between accounts
- **Validation**: Comprehensive input validation for all endpoints
- **Error Handling**: Detailed error responses for client applications
- **Database**: SQLite persistence with SQLx for type-safe queries
- **Rate Limiting**: IP-based rate limiting to prevent abuse and ensure service stability

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
- `GET /api/protected/transactions/list` - List user's transactions

### Users (Protected Routes)

- `GET /api/protected/users/me` - Get current user info
- `GET /api/protected/users/account` - Get user's account details

## Getting Started

### Prerequisites

- Rust 1.65+ and Cargo
- SQLite 3.39+

### Environment Variables

Create a `.env` file in the project root:

```env
DATABASE_URL="sqlite:data.db"
JWT_SECRET="asjthilak"
PORT=3000
GLOBAL_RATE_LIMIT=300
AUTH_RATE_LIMIT=30
```

- `GLOBAL_RATE_LIMIT`: Maximum requests allowed per minute globally (default: 300)
- `AUTH_RATE_LIMIT`: Maximum requests allowed per minute for authentication endpoints (default: 30)

### Running Locally

```bash
# Install dependencies & build the project
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


## Project Structure

```
payment-system/
├── src/
│   ├── db/           # Database operations and schema
│   ├── handlers/     # API endpoint handlers
│   ├── middleware/   # Auth middleware and JWT validation
│   ├── models/       # Data models and validation
│   ├── lib.rs        # Library exports
│   └── main.rs       # Application entry point
├── migrations/       # SQLx database migrations
├── tests/            # Integration tests
└── Dockerfile        # Container configuration
```

## License

This project is licensed under the MIT License.