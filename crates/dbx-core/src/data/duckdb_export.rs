use tokio_util::sync::CancellationToken;

use crate::connection::AppState;
use crate::models::connection::DatabaseType;
use crate::query::{execute_sql_statement_with_options, QueryExecutionOptions};
use crate::types::QueryResult;

/// A single execution of an export query, stored in DuckDB's temporary catalog.
/// Independent LIMIT/OFFSET queries can overlap when ORDER BY contains ties,
/// including when preserve_insertion_order is enabled.
pub(super) struct DuckDbExportSnapshot {
    table: String,
    columns: Vec<String>,
    connection_id: String,
    database: String,
    schema: Option<String>,
    options: QueryExecutionOptions,
    pool_key: Option<String>,
}

impl DuckDbExportSnapshot {
    pub(super) fn new(
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        options: QueryExecutionOptions,
    ) -> Self {
        Self {
            table: format!("__dbx_export_{}", uuid::Uuid::new_v4().simple()),
            columns: Vec::new(),
            connection_id: connection_id.to_string(),
            database: database.to_string(),
            schema: schema.map(str::to_string),
            options,
            pool_key: None,
        }
    }

    fn qualified_table(&self) -> String {
        format!("\"temp\".\"main\".\"{}\"", self.table)
    }

    async fn execute(
        &self,
        state: &AppState,
        sql: &str,
        max_rows: usize,
        cancel_token: Option<CancellationToken>,
    ) -> Result<QueryResult, String> {
        let mut options = self.options.clone();
        options.max_rows = Some(max_rows);
        execute_sql_statement_with_options(
            state,
            &self.connection_id,
            &self.database,
            sql,
            self.schema.as_deref(),
            cancel_token,
            options,
        )
        .await
    }

    /// These statements only create/drop our generated temporary table. Keep
    /// them off the user-SQL write guard, after validating the source query,
    /// so read-only connections can still export without modifying source data.
    async fn execute_temporary_ddl(
        &self,
        state: &AppState,
        sql: &str,
        cancel_token: Option<CancellationToken>,
        cleanup: bool,
    ) -> Result<(), String> {
        #[cfg(feature = "duckdb-sidecar")]
        {
            let pool_key = self.pool_key.as_deref().ok_or("DuckDB export connection is not initialized")?;
            let Some(crate::connection::PoolKind::DuckDbWorker(client)) = state.pool_handle(pool_key).await else {
                return Err("DuckDB export connection is closed".into());
            };
            let timeout_secs = if cleanup {
                5
            } else if let Some(seconds) = self.options.timeout_secs {
                seconds
            } else {
                state.configs.read().await.get(&self.connection_id).map_or(0, |config| config.query_timeout_secs)
            };
            let timeout = (timeout_secs > 0).then(|| std::time::Duration::from_secs(timeout_secs));
            client.execute(Some(self.database.clone()), sql.to_string(), Some(1), cancel_token, timeout).await?;
            Ok(())
        }
        #[cfg(not(feature = "duckdb-sidecar"))]
        {
            let _ = (state, sql, cancel_token, cleanup);
            Err("DuckDB worker support is not compiled in this build".into())
        }
    }

    pub(super) async fn materialize(
        &mut self,
        state: &AppState,
        sql: &str,
        row_limit: Option<usize>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<(), String> {
        let statements = crate::sql::split_sql_statements_for_database(sql, DatabaseType::DuckDb);
        if statements.len() != 1 {
            return Err("DuckDB export requires a single query".to_string());
        }
        let sql = &statements[0];
        let pool_key = state
            .get_or_create_pool_for_session(
                &self.connection_id,
                Some(&self.database),
                self.options.client_session_id.as_deref(),
            )
            .await?;
        crate::query::check_read_only_for_connection(state, &pool_key, sql).await?;
        self.pool_key = Some(pool_key);
        // DESCRIBE binds without running the query. Internal names preserve
        // duplicate result labels and user columns named rowid in the output.
        let metadata =
            self.execute(state, &format!("DESCRIBE (\n{sql}\n)"), i32::MAX as usize, cancel_token.clone()).await?;
        if metadata.truncated {
            return Err("DuckDB export column metadata was truncated".to_string());
        }
        self.columns = metadata
            .rows
            .iter()
            .map(|row| {
                row.first()
                    .and_then(|cell| cell.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| "DuckDB export returned invalid column metadata".to_string())
            })
            .collect::<Result<_, _>>()?;
        if self.columns.is_empty() {
            return Err("DuckDB export query returned no columns".to_string());
        }
        let aliases = (0..self.columns.len()).map(|i| format!("\"__dbx_column_{i}\"")).collect::<Vec<_>>().join(", ");
        let limit = row_limit.map(|limit| format!(" LIMIT {limit}")).unwrap_or_default();
        // Capture output order before materialization, independently of how
        // DuckDB subsequently stores temporary-table blocks in parallel.
        let create = format!(
            "CREATE TEMP TABLE \"{}\" (\"__dbx_row_number\", {aliases}) AS \
             SELECT row_number() OVER (), * FROM (\n{sql}\n) AS \"__dbx_source\"{limit}",
            self.table
        );
        self.execute_temporary_ddl(state, &create, cancel_token, false).await?;
        Ok(())
    }

    pub(super) async fn page(
        &self,
        state: &AppState,
        offset: u64,
        limit: usize,
        cancel_token: Option<CancellationToken>,
    ) -> Result<QueryResult, String> {
        let end = offset.saturating_add(limit as u64);
        let projection = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| format!("\"__dbx_column_{i}\" AS \"{}\"", column.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {projection} FROM {} WHERE \"__dbx_row_number\" > {offset} \
             AND \"__dbx_row_number\" <= {end} ORDER BY \"__dbx_row_number\"",
            self.qualified_table()
        );
        self.execute(state, &sql, limit, cancel_token).await
    }

    pub(super) async fn close(self, state: &AppState) {
        // Cleanup must not inherit a cancelled execution id. The qualified name
        // can only refer to this export's temporary table.
        if self.pool_key.is_none() {
            return;
        }
        let sql = format!("DROP TABLE IF EXISTS {}", self.qualified_table());
        if let Err(error) = self.execute_temporary_ddl(state, &sql, None, true).await {
            log::warn!("Failed to remove DuckDB export snapshot: {error}");
        }
    }
}
