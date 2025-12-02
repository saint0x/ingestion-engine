//! ClickHouse health checks.

use crate::client::ClickHouseClient;
use tracing::{debug, error};

/// Check ClickHouse connection health.
pub async fn check_connection(client: &ClickHouseClient) -> bool {
    match client.inner().query("SELECT 1").fetch_one::<u8>().await {
        Ok(_) => {
            debug!("ClickHouse connection healthy");
            true
        }
        Err(e) => {
            error!("ClickHouse health check failed: {}", e);
            false
        }
    }
}

/// Initialize database schema.
pub async fn init_schema(client: &ClickHouseClient) -> Result<(), String> {
    use crate::schema::all_tables;

    for ddl in all_tables() {
        client
            .inner()
            .query(ddl)
            .execute()
            .await
            .map_err(|e| format!("Failed to execute DDL: {}", e))?;
    }

    debug!("ClickHouse schema initialized");
    Ok(())
}
