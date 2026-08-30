/// Database utility functions for tests
use sea_orm::DatabaseConnection;

/// Database test utilities
pub struct DbTestUtils;

impl DbTestUtils {
    /// Clean up test data from database
    ///
    /// # Errors
    ///
    /// Returns a [`sea_orm::DbErr`] if cleanup fails. The current implementation
    /// always returns `Ok(())`.
    #[allow(clippy::unused_async)] // Public API kept async for future I/O cleanup.
    pub async fn cleanup_test_data(_db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
        // Add cleanup logic if needed
        Ok(())
    }
}
