use super::{registry::CommandRegistry, Command, CommandContext, CommandError};
use std::sync::Arc;

/// Generic command service that works with any registered commands
pub struct GenericCommandService {
    registry: Arc<CommandRegistry>,
}

impl GenericCommandService {
    /// Create a new generic command service
    #[must_use]
    pub const fn new(registry: Arc<CommandRegistry>) -> Self {
        Self { registry }
    }

    /// Execute any command that's registered in the registry
    pub async fn execute<C: Command + Clone + 'static>(
        &self,
        command: C,
        context: CommandContext,
    ) -> Result<C::Result, CommandError> {
        self.registry.execute_command(command, context).await
    }

    /// List all available command types
    #[must_use]
    pub fn list_available_commands(&self) -> Vec<String> {
        self.registry.list_command_types()
    }

    /// Check if a command type is supported
    #[must_use]
    pub fn supports_command(&self, command_type: &str) -> bool {
        self.registry.get_handler(command_type).is_some()
    }
}

impl Clone for GenericCommandService {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{CommandErrorMapper, CommandRegistryBuilder},
        CommandHandler,
    };
    use async_trait::async_trait;
    use std::sync::Arc;
    use uuid::Uuid;

    // Test command implementation
    #[derive(Debug, Clone)]
    struct TestCommand {
        id: Uuid,
        data: String,
    }

    impl TestCommand {
        fn new(data: String) -> Self {
            Self {
                id: Uuid::new_v4(),
                data,
            }
        }
    }

    impl Command for TestCommand {
        type Result = String;

        fn command_type(&self) -> &'static str {
            "test_command"
        }

        fn command_id(&self) -> Uuid {
            self.id
        }

        fn validate(&self) -> Result<(), CommandError> {
            if self.data.is_empty() {
                Err(CommandError::validation(
                    "empty_data",
                    "Data cannot be empty",
                ))
            } else {
                Ok(())
            }
        }
    }

    // Test handler implementation
    struct TestHandler;

    #[async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        async fn handle(&self, command: TestCommand) -> Result<String, CommandError> {
            Ok(format!("Processed: {}", command.data))
        }
    }

    // Test error mapper implementation
    struct TestErrorMapper;

    impl CommandErrorMapper for TestErrorMapper {
        fn map_error(&self, error: Box<dyn std::error::Error + Send + Sync>) -> CommandError {
            CommandError::infrastructure("test_error", error.to_string())
        }
    }

    #[tokio::test]
    async fn test_generic_service_execution() {
        let handler = Arc::new(TestHandler);
        let error_mapper = Arc::new(TestErrorMapper);

        let registry = CommandRegistryBuilder::new()
            .register::<TestCommand, _>("test_command".to_string(), handler, error_mapper)
            .build();

        let service = GenericCommandService::new(Arc::new(registry));
        let command = TestCommand::new("test data".to_string());
        let context = CommandContext::new();

        let result = service.execute(command, context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Processed: test data");
    }

    #[test]
    fn test_generic_service_lists_commands() {
        let handler = Arc::new(TestHandler);
        let error_mapper = Arc::new(TestErrorMapper);

        let registry = CommandRegistryBuilder::new()
            .register::<TestCommand, _>("test_command".to_string(), handler, error_mapper)
            .build();

        let service = GenericCommandService::new(Arc::new(registry));
        let commands = service.list_available_commands();

        assert_eq!(commands, vec!["test_command"]);
    }

    #[test]
    fn test_generic_service_supports_command() {
        let handler = Arc::new(TestHandler);
        let error_mapper = Arc::new(TestErrorMapper);

        let registry = CommandRegistryBuilder::new()
            .register::<TestCommand, _>("test_command".to_string(), handler, error_mapper)
            .build();

        let service = GenericCommandService::new(Arc::new(registry));

        assert!(service.supports_command("test_command"));
        assert!(!service.supports_command("unknown_command"));
    }

    #[test]
    fn test_generic_service_clone() {
        let handler = Arc::new(TestHandler);
        let error_mapper = Arc::new(TestErrorMapper);

        let registry = CommandRegistryBuilder::new()
            .register::<TestCommand, _>("test_command".to_string(), handler, error_mapper)
            .build();

        let service = GenericCommandService::new(Arc::new(registry));
        let cloned_service = service;

        assert!(cloned_service.supports_command("test_command"));
    }

    #[tokio::test]
    async fn test_generic_service_validation_error() {
        let handler = Arc::new(TestHandler);
        let error_mapper = Arc::new(TestErrorMapper);

        let registry = CommandRegistryBuilder::new()
            .register::<TestCommand, _>("test_command".to_string(), handler, error_mapper)
            .build();

        let service = GenericCommandService::new(Arc::new(registry));
        let command = TestCommand::new(String::new()); // Empty data should fail validation
        let context = CommandContext::new();

        let result = service.execute(command, context).await;
        assert!(result.is_err());

        if let Err(CommandError::Validation { code: _, message }) = result {
            assert!(message.contains("Data cannot be empty"));
        } else {
            panic!("Expected validation error");
        }
    }
}
