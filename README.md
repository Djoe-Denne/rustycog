# RustyCog SDK

A comprehensive SDK for building microservices in Rust, extracted from production-ready patterns and best practices.

## Overview

RustyCog provides a set of crates that handle the common concerns of microservice development, allowing you to focus on your business logic. It's designed around clean architecture principles with clear separation of concerns.

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
- **Error Mapping Macros**: Automatic domain error to HTTP error conversion
- **Command Macros**: Boilerplate reduction for command implementation
- **Type Safety**: Compile-time guarantees for common patterns

## Quick Start

### 1. Add RustyCog to your project

```toml
[dependencies]
rustycog-core = "0.1"
rustycog-macros = "0.1"
rustycog-server = "0.1"
rustycog-http = "0.1"
```

### 2. Define your domain errors

```rust
use rustycog_macros::ErrorMapper;

#[derive(Debug, Clone, thiserror::Error, ErrorMapper)]
#[error_mapper(domain = "user")]
pub enum UserError {
    #[error("User not found")]
    #[error_mapper(status = 404, category = "not_found")]
    UserNotFound,
    
    #[error("User already exists")]
    #[error_mapper(status = 409, category = "conflict")]
    UserAlreadyExists,
    
    #[error("Invalid email format")]
    #[error_mapper(status = 400, category = "validation")]
    InvalidEmail,
}
```

### 3. Create commands

```rust
use rustycog_core::{Command, CommandContext};

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
    
    fn validate(&self) -> Result<(), ServiceError> {
        if self.email.is_empty() {
            return Err(ServiceError::validation_field("Email required", "email"));
        }
        Ok(())
    }
    
    fn context(&self) -> &CommandContext { &self.context }
    fn set_context(&mut self, context: CommandContext) { self.context = context; }
}
```

### 4. Implement your service

```rust
pub struct UserService {
    repository: Arc<dyn UserRepository>,
    event_publisher: Arc<dyn EventPublisher>,
}

impl UserService {
    pub async fn create_user(&self, command: CreateUserCommand) -> Result<User, UserError> {
        command.validate()?;
        
        // Business logic here
        let user = User::new(command.username, command.email);
        
        // Save to database
        self.repository.save(user.clone()).await?;
        
        // Publish domain event
        let event = UserCreatedEvent::new(user.id);
        self.event_publisher.publish(Box::new(event)).await?;
        
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

## Crates

### `rustycog-core`
Core abstractions and traits that all other crates depend on.

### `rustycog-macros`
Procedural macros for reducing boilerplate:
- `#[derive(ErrorMapper)]` - Automatic error mapping
- `#[derive(Command)]` - Command trait implementation

### `rustycog-server`
Server setup and application bootstrapping utilities.

### `rustycog-command`
Generic command pattern implementation with:
- Command registry and routing
- Retry policies and circuit breakers
- Metrics collection
- Validation framework

### `rustycog-db`
Database management utilities:
- Connection pooling with read/write split
- Migration management
- Repository pattern implementations

### `rustycog-events`
Event publishing and subscription:
- Kafka integration
- Domain event patterns
- Test utilities

### `rustycog-http`
HTTP server utilities:
- Axum integration
- Error response formatting
- Middleware
- Request/response validation

### `rustycog-config`
Configuration management:
- Environment-based configuration
- Validation
- Type-safe configuration structs

### `rustycog-testing`
Testing utilities:
- Test containers for databases and Kafka
- HTTP testing helpers
- Fixture management

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
    fn validate(&self) -> Result<(), ServiceError> { /* ... */ }
}

// 3. Handle the command
impl CommandHandler<CreateUserCommand> for UserService {
    async fn handle(&self, command: CreateUserCommand) -> Result<User, ServiceError> {
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
let config = rustycog_config::load::<AppConfig>()?;
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

See the `examples/` directory for complete working examples:

- **simple-service**: Basic CRUD service
- **event-driven-service**: Service with event publishing
- **microservice-template**: Full-featured microservice template

## Migration from Existing Code

RustyCog is designed to be incrementally adoptable. You can start by:

1. **Add error mapping**: Use `#[derive(ErrorMapper)]` on existing error enums
2. **Introduce commands**: Wrap existing operations in command structs
3. **Add structured configuration**: Replace ad-hoc config with `rustycog-config`
4. **Improve testing**: Use test containers and HTTP testing utilities

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

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
