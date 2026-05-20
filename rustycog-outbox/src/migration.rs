use sea_orm_migration::prelude::*;

#[must_use]
pub fn outbox_migration() -> Box<dyn MigrationTrait> {
    Box::new(Migration)
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &'static str {
        "m20260426_000001_create_rustycog_outbox_events"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OutboxEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OutboxEvents::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(OutboxEvents::EventId)
                            .uuid()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(OutboxEvents::EventType).text().not_null())
                    .col(ColumnDef::new(OutboxEvents::AggregateId).uuid().not_null())
                    .col(ColumnDef::new(OutboxEvents::Version).integer().not_null())
                    .col(
                        ColumnDef::new(OutboxEvents::OccurredAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OutboxEvents::PayloadJson)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OutboxEvents::MetadataJson)
                            .json_binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(OutboxEvents::Status).text().not_null())
                    .col(
                        ColumnDef::new(OutboxEvents::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(OutboxEvents::NextAttemptAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(OutboxEvents::LockedBy).text())
                    .col(ColumnDef::new(OutboxEvents::LockedUntil).timestamp_with_time_zone())
                    .col(ColumnDef::new(OutboxEvents::LastError).text())
                    .col(
                        ColumnDef::new(OutboxEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(OutboxEvents::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_rustycog_outbox_events_claim")
                    .table(OutboxEvents::Table)
                    .col(OutboxEvents::Status)
                    .col(OutboxEvents::NextAttemptAt)
                    .col(OutboxEvents::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_rustycog_outbox_events_aggregate")
                    .table(OutboxEvents::Table)
                    .col(OutboxEvents::AggregateId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(OutboxEvents::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

enum OutboxEvents {
    Table,
    Id,
    EventId,
    EventType,
    AggregateId,
    Version,
    OccurredAt,
    PayloadJson,
    MetadataJson,
    Status,
    Attempts,
    NextAttemptAt,
    LockedBy,
    LockedUntil,
    LastError,
    CreatedAt,
    UpdatedAt,
}

impl Iden for OutboxEvents {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        let ident = match self {
            Self::Table => "rustycog_outbox_events",
            Self::Id => "id",
            Self::EventId => "event_id",
            Self::EventType => "event_type",
            Self::AggregateId => "aggregate_id",
            Self::Version => "version",
            Self::OccurredAt => "occurred_at",
            Self::PayloadJson => "payload_json",
            Self::MetadataJson => "metadata_json",
            Self::Status => "status",
            Self::Attempts => "attempts",
            Self::NextAttemptAt => "next_attempt_at",
            Self::LockedBy => "locked_by",
            Self::LockedUntil => "locked_until",
            Self::LastError => "last_error",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
        };

        if s.write_str(ident).is_err() {}
    }
}
