//! Query functions for reading data back (used in tests and admin).

use crate::client::ClickHouseClient;
use clickhouse::Row;
use engine_core::Result;
use serde::Deserialize;

/// Query result for event verification.
#[derive(Debug, Clone, Row, Deserialize)]
pub struct QueryEventRow {
    pub event_id: String,
    pub project_id: String,
    pub session_id: String,
    pub user_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub url: String,
    pub path: String,
}

/// Count events for a project.
pub async fn count_events(client: &ClickHouseClient, project_id: &str) -> Result<u64> {
    let count: u64 = client
        .inner()
        .query("SELECT count() FROM overwatch.events WHERE project_id = ?")
        .bind(project_id)
        .fetch_one()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;
    Ok(count)
}

/// Count all events in the table (for testing).
pub async fn count_all_events(client: &ClickHouseClient) -> Result<u64> {
    let count: u64 = client
        .inner()
        .query("SELECT count() FROM overwatch.events")
        .fetch_one()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;
    Ok(count)
}

/// Fetch events for a project (for verification).
pub async fn query_events(
    client: &ClickHouseClient,
    project_id: &str,
    limit: u32,
) -> Result<Vec<QueryEventRow>> {
    let rows: Vec<QueryEventRow> = client
        .inner()
        .query("SELECT event_id, project_id, session_id, user_id, type, url, path FROM overwatch.events WHERE project_id = ? ORDER BY timestamp DESC LIMIT ?")
        .bind(project_id)
        .bind(limit)
        .fetch_all()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;
    Ok(rows)
}

/// Fetch all events (for testing).
pub async fn query_all_events(client: &ClickHouseClient, limit: u32) -> Result<Vec<QueryEventRow>> {
    let rows: Vec<QueryEventRow> = client
        .inner()
        .query("SELECT event_id, project_id, session_id, user_id, type, url, path FROM overwatch.events ORDER BY timestamp DESC LIMIT ?")
        .bind(limit)
        .fetch_all()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;
    Ok(rows)
}

/// Delete all events for a project (test cleanup).
pub async fn delete_project_events(client: &ClickHouseClient, project_id: &str) -> Result<()> {
    client
        .inner()
        .query("ALTER TABLE overwatch.events DELETE WHERE project_id = ?")
        .bind(project_id)
        .execute()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Delete error: {}", e)))?;
    Ok(())
}

/// Truncate all events (test cleanup).
pub async fn truncate_events(client: &ClickHouseClient) -> Result<()> {
    client
        .inner()
        .query("TRUNCATE TABLE IF EXISTS overwatch.events")
        .execute()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Truncate error: {}", e)))?;
    Ok(())
}
