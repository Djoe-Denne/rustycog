# RustyCog SDK

A feature-gated SDK for building microservices in Rust, extracted from production-ready patterns and best practices.

## Overview

RustyCog provides one runtime package, `rustycog-framework`, that handles common microservice concerns through feature-gated modules. Consumers typically alias the package as `rustycog` in `Cargo.toml`, then import modules such as `rustycog::core`, `rustycog::config`, `rustycog::http`, and `rustycog::events`.

The historical `rustycog-*` crate names now describe module/reference boundaries inside the unified runtime package. Integration-test helpers remain separate in the `rustycog-testing` package.

## Features

### 🎯 **Core Abstractions**
- **Command Pattern**: Type-safe command handling with validation and metrics
- **Error Management**: Structured error handling with automatic HTTP mapping
- **Event System**: Domain event publishing and subscription
- **Repository Pattern**: CQRS-ready repository abstractions

### 🚀 **Server Setup**
- **Application Bootstrapping**: Generic server setup with dependency injection
- **Configuration Management**: Environment-based configuration with validation
- **Health Checks**: Built-in health check endpoints
- **Graceful Shutdown**: Proper resource cleanup

### 🗄️ **Database Management**
- **Connection Pooling**: Read/write split with automatic failover
- **Migration Support**: Database schema versioning
- **Transaction Management**: Automatic transaction handling

### 📨 **Event Publishing**
- **Kafka Integration**: Production-ready Kafka publisher/subscriber
- **Event Sourcing**: Domain event patterns
- **Test Support**: In-memory event handling for testing

### 🧪 **Testing Framework**
- **Test Containers**: Automatic database and Kafka setup
- **HTTP Testing**: Axum-based API testing utilities
- **Fixture Management**: Reusable test data

### 🔧 **Developer Experience**
- **Error Mapping Helpers**: Consistent domain error to HTTP error conversion
- **Command Runtime**: Reusable command registry and execution pipeline
- **Type Safety**: Compile-time guarantees for common patterns

## Quick Start

### 1. Add RustyCog to your project

```toml
[dependencies]
rustycog = { package = "rustycog-framework", version = "0.1", default-features = false, features = [
    "core",
    "config",
    "command",
    "events",
    "http",
] }

[dev-dependencies]
rustycog-testing = "0.1"
```

For broad application crates, use `features = ["full"]`. For libraries, prefer the smallest feature set that exposes the modules you actually use.

### 2. Define your domain errors

```rust
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum UserError {
    #[error("User not found")]
    UserNotFound,

    #[error("User already exists")]
    UserAlreadyExists,

    #[error("Invalid email format")]
    InvalidEmail,
}
```

### 3. Create commands

```rust
use rustycog::command::{Command, CommandContext, CommandError};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateUserCommand {
    pub context: CommandContext,
    pub username: String,
    pub email: String,
}

impl Command for CreateUserCommand {
    type Result = User;
    
    fn command_type(&self) -> &'static str { "create_user" }
    fn command_id(&self) -> Uuid { self.context.execution_id }
    
    fn validate(&self) -> Result<(), CommandError> {
        if self.email.is_empty() {
            return Err(CommandError::validation("email_required", "Email required"));
        }
        Ok(())
    }
}
```

### 4. Implement your service

```rust
use rustycog::events::EventPublisher;
use rustycog::core::error::ServiceError;
use std::sync::Arc;

pub struct UserService {
    repository: Arc<dyn UserRepository>,
    event_publisher: Arc<dyn EventPublisher<ServiceError>>,
}

impl UserService {
    pub async fn create_user(&self, command: CreateUserCommand) -> Result<User, ServiceError> {
        command
            .validate()
            .map_err(|err| ServiceError::validation(err.message()))?;
        
        // Business logic here
        let user = User::new(command.username, command.email);
        
        // Save to database
        self.repository.save(user.clone()).await?;
        
        // Publish domain event
        let event = UserCreatedEvent::new(user.id);
        self.event_publisher.publish(&event).await?;
        
        Ok(user)
    }
}
```

## Architecture

RustyCog follows clean architecture principles:

```
┌─────────────────────────────────────────────────────────────┐
│                     HTTP Layer                              │
│  (Controllers, Middleware, Error Handling)                 │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                 Application Layer                           │
│     (Commands, Use Cases, Application Services)            │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                   Domain Layer                              │
│        (Entities, Value Objects, Domain Services)          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                Infrastructure Layer                         │
│    (Database, Kafka, External Services, Repositories)      │
└─────────────────────────────────────────────────────────────┘
```

## Feature Modules

The runtime surface is one package with optional modules:

### `rustycog::core` (`core`)
Core error contracts shared by the other modules.

### `rustycog::command` (`command`)
Generic command pattern implementation with:
- Command registry and routing
- Retry policies and circuit breakers
- Metrics collection
- Validation framework

### `rustycog::config` (`config`)
Configuration management:
- Environment-based configuration
- Validation
- Type-safe configuration structs

### `rustycog::db` (`db`)
Database management utilities:
- Connection pooling with read/write split
- Migration management
- Repository pattern implementations

### `rustycog::events` (`events`)
Event publishing and subscription:
- Kafka integration
- SQS integration
- Domain event patterns

### `rustycog::http` (`http`)
HTTP server utilities:
- Axum integration
- Error response formatting
- Middleware
- Request/response validation

### `rustycog::permission` (`permission`)
Authorization primitives and OpenFGA-backed permission checks.

### `rustycog::outbox` (`outbox`)
Transactional outbox support for durable event dispatch.

### `rustycog::logger` (`logger`)
Tracing and logging initialization helpers.

### `rustycog::server` (`server`)
Health-check abstractions.

### `rustycog-testing`
Separate testing package with testcontainers, HTTP helpers, wiremock fixtures, and real infrastructure bootstrap utilities.

## Error Handling Philosophy

RustyCog promotes a structured approach to error handling:

1. **Domain Errors**: Business logic errors specific to your domain
2. **Service Errors**: Generic application-level errors
3. **HTTP Errors**: Automatically mapped HTTP responses

```rust
// Domain error
UserError::UserNotFound 
    ↓ (automatic mapping)
// Service error  
ServiceError::NotFound { message: "user: User not found", .. }
    ↓ (automatic mapping)
// HTTP response
404 Not Found
{
  "error": {
    "error_code": "not_found",
    "message": "user: User not found",
    "status": 404
  }
}
```

## Command Pattern

Commands provide a consistent way to handle business operations:

```rust
// 1. Define the command
struct CreateUserCommand { /* ... */ }

// 2. Implement validation
impl Command for CreateUserCommand {
    fn validate(&self) -> Result<(), CommandError> { /* ... */ }
}

// 3. Handle the command
impl CommandHandler<CreateUserCommand> for UserService {
    async fn handle(&self, command: CreateUserCommand) -> Result<User, CommandError> {
        // Business logic here
    }
}

// 4. Execute with automatic metrics and error handling
let result = command_service.execute(create_user_command).await;
```

## Configuration

RustyCog uses environment-based configuration:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub kafka: KafkaConfig,
    pub logging: LoggingConfig,
}

// Automatically loaded from environment variables or config files
let config = rustycog::config::load_config_fresh::<AppConfig>()?;
```

## Testing

RustyCog provides comprehensive testing utilities:

```rust
#[tokio::test]
async fn test_create_user() {
    // Automatic test database setup
    let test_db = TestDatabase::new().await;
    
    // Automatic Kafka test container
    let test_kafka = TestKafka::new().await;
    
    // HTTP testing
    let app = create_test_app(test_db.pool(), test_kafka.publisher()).await;
    let server = TestServer::new(app);
    
    let response = server
        .post("/users")
        .json(&CreateUserRequest {
            username: "test_user".to_string(),
            email: "test@example.com".to_string(),
        })
        .await;
    
    response.assert_status_ok();
    response.assert_json(&expected_user);
}
```

## Examples

Reference implementations currently live in this repository's service consumers
(`IAMRusty`, `Hive`, `Manifesto`, `Telegraph`) and in the integration tests
that exercise the unified `rustycog-framework` package plus `rustycog-testing`.

## Migration from Existing Code

RustyCog is designed to be incrementally adoptable. You can start by:

1. **Standardize domain errors**: Start with `thiserror` enums and map them at the HTTP boundary
2. **Introduce commands**: Wrap existing operations in command structs
3. **Add structured configuration**: Replace ad-hoc config with `rustycog::config`
4. **Improve testing**: Use test containers and HTTP testing utilities

## Contributing

We welcome contributions through issues and pull requests on GitHub.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Inspiration

RustyCog is extracted from real-world production microservices and incorporates patterns from:

- Clean Architecture (Robert C. Martin)
- Domain-Driven Design (Eric Evans)
- Command Query Responsibility Segregation (CQRS)
- Event Sourcing patterns
- Microservice architecture best practices # rustycog
