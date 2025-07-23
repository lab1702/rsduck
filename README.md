# RSDuck 🦆

A high-performance REST API server for DuckDB written in Rust. RSDuck provides a simple HTTP interface to interact with DuckDB databases, supporting both in-memory and file-based databases with configurable read/write permissions.

## Features

- 🚀 **High Performance**: Built with Rust and Tokio for excellent concurrency
- 🦆 **DuckDB Integration**: Direct integration with DuckDB for analytical workloads
- 🔒 **Access Control**: Read-only and read-write modes for security
- 📁 **Flexible Storage**: Support for both in-memory and file-based databases
- 🌐 **REST API**: Clean HTTP endpoints for all database operations
- ⚡ **Concurrent**: Handle multiple simultaneous requests
- 🛡️ **Error Handling**: Comprehensive error responses with proper HTTP status codes

## Quick Start

### Prerequisites

- Rust 1.70+ installed
- Git

### Installation

```bash
git clone <repository-url>
cd rsduck
cargo build --release
```

### Basic Usage

```bash
# Start with in-memory database
cargo run

# Use a file-based database (read-only)
cargo run -- --database mydata.duckdb

# Use a file-based database (read-write)
cargo run -- --database mydata.duckdb --readwrite

# Custom port and host
cargo run -- --port 8080 --host 127.0.0.1
```

## Command Line Options

```
A DuckDB REST server

Usage: rsduck [OPTIONS]

Options:
  -d, --database <DATABASE>  DuckDB database file path (uses in-memory database if not specified)
      --readwrite            Open database in read-write mode (default is read-only for file databases)
  -p, --port <PORT>          Server port [default: 3001]
      --host <HOST>          Server host [default: 0.0.0.0]
  -h, --help                 Print help
  -V, --version              Print version
```

## API Documentation

### Base URL

When running locally: `http://localhost:3001`

### Endpoints

#### Health Check

**GET** `/health`

Returns server status and database information.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": 1753239312,
  "database_path": "mydata.duckdb",
  "readonly_mode": false
}
```

#### Execute Query (POST)

**POST** `/query`

Execute SQL queries using JSON request body.

**Request:**
```json
{
  "sql": "SELECT * FROM users WHERE age > 18"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "rows": [
      ["Alice", 25],
      ["Bob", 30]
    ],
    "row_count": 2,
    "rows_affected": 0
  },
  "error": null,
  "query_id": "uuid-here",
  "execution_time_ms": 15
}
```

#### Execute Query (GET)

**GET** `/query?sql=<encoded-sql>`

Execute SQL queries using URL parameters.

**Example:**
```bash
curl "http://localhost:3001/query?sql=SELECT%20COUNT(*)%20FROM%20users"
```

### Example Requests

#### Create Table
```bash
curl -X POST http://localhost:3001/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE TABLE users (id INT, name TEXT, age INT)"}'
```

#### Insert Data
```bash
curl -X POST http://localhost:3001/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO users VALUES (1, '\''Alice'\'', 25), (2, '\''Bob'\'', 30)"}'
```

#### Select Data
```bash
curl -X POST http://localhost:3001/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM users WHERE age > 18"}'
```

#### Complex Analytics
```bash
curl -X POST http://localhost:3001/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT age, COUNT(*) as count FROM users GROUP BY age ORDER BY age"}'
```

## Usage Patterns

### Development Mode

For development and testing, use an in-memory database:

```bash
cargo run
```

This provides a clean slate each time and doesn't persist data.

### Production File Database

For production use with persistent data:

```bash
# Start in read-write mode for initial setup
cargo run -- --database prod.duckdb --readwrite

# Switch to read-only mode for query-only access
cargo run -- --database prod.duckdb
```

### Analytics Workstation

Perfect for data analysis workflows:

```bash
# Load your data file
cargo run -- --database analytics.duckdb --readwrite

# Run complex analytical queries via HTTP
curl -X POST http://localhost:3001/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT region, SUM(sales) FROM revenue GROUP BY region"}'
```

## Security Considerations

### Read-Only Mode

File databases open in read-only mode by default. Write operations will return HTTP 403:

```json
{
  "success": false,
  "error": "Database is opened in read-only mode. Write operations are not allowed.",
  "query_id": "...",
  "execution_time_ms": 0
}
```

### Access Control

- Use `--readwrite` flag only when write access is needed
- Consider running read-only instances for query-only users
- File permissions should be set appropriately on the database file

## Error Handling

RSDuck provides detailed error responses:

- **400 Bad Request**: Invalid SQL or missing parameters
- **403 Forbidden**: Write operation on read-only database
- **500 Internal Server Error**: Database connection or server issues

Example error response:
```json
{
  "success": false,
  "data": null,
  "error": "Catalog Error: Table with name 'nonexistent' does not exist!",
  "query_id": "uuid-here",
  "execution_time_ms": 5
}
```

## Performance

RSDuck is built for performance:

- **Async Runtime**: Uses Tokio for handling concurrent requests
- **Connection Pooling**: Efficient database connection management
- **Fast JSON**: Optimized serialization with serde
- **Low Latency**: Typical response times under 10ms for simple queries

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## Dependencies

- **axum**: Web framework
- **tokio**: Async runtime
- **duckdb**: Database engine
- **serde**: JSON serialization
- **clap**: Command line parsing
- **uuid**: Query ID generation

## License

[Add your license here]

## Troubleshooting

### Port Already in Use
```
Error: Address already in use (os error 98)
```
Use a different port: `cargo run -- --port 3002`

### Database File Locked
```
Error: database is locked
```
Ensure no other processes are using the database file.

### Permission Denied
```
Error: Permission denied
```
Check file permissions on the database file and directory.

### Build Issues
```
error: linking with `cc` failed
```
Install build dependencies: `sudo apt-get install build-essential`

---

Built with ❤️ and 🦆 DuckDB