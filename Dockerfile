# Build stage
FROM rust:latest as builder

WORKDIR /app
COPY . .

# Install SQLite development files needed for compilation
RUN apt-get update && \
    apt-get install -y libsqlite3-dev && \
    rm -rf /var/lib/apt/lists/*

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y libsqlite3-0 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary and migrations
COPY --from=builder /app/target/release/payment_system /app/
COPY --from=builder /app/migrations /app/migrations
COPY data.db /app/data.db

# Set environment variables
ENV DATABASE_URL=sqlite:data.db
# JWT_SECRET should be set at runtime via environment variable
# Default value is provided for convenience in development only
ENV JWT_SECRET=changeme_in_production
ENV PORT=3000

# Expose the port
EXPOSE 3000

# Run the application
CMD ["./payment_system"]