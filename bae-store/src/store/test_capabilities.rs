use crate::{DbError, Store};
use coven::ClockRef;

impl Store {
    /// This store with `clock` stamping its writes, for a test whose later
    /// writes happen at another time than its first.
    pub fn under_clock_for_test(&self, clock: ClockRef) -> Self {
        Self::new(self.inner.handle.clone(), clock, self.inner.bugs.clone())
    }

    /// Count the work this store's connections run from here on into the
    /// counter returned: what a test of a write's or a read's cost measures.
    pub async fn track_work_for_test(&self) -> Result<coven::WorkCounter, DbError> {
        let counter = coven::WorkCounter::new();
        self.inner.handle.test_track_work(counter.clone()).await.map_err(DbError::from)?;
        Ok(counter)
    }

    /// Record the statements this store's reads and live queries run from
    /// here on, in order with their bound values written in, until the log
    /// returned drops. The log does not record writes.
    pub async fn record_statements_for_test(&self) -> Result<coven::StatementLog, DbError> {
        let log = coven::StatementLog::new();
        self.inner.handle.test_record_statements(log.clone()).await.map_err(DbError::from)?;
        Ok(log)
    }

    pub fn store_id_for_test(&self) -> coven::StoreId {
        self.store_id()
    }

    /// Every row each write still waiting to upload records, oldest write
    /// first: a row it changes as its table's name, and a row its SQL touched
    /// with no net change as `"<table> (unchanged)"`. Sorted within a write.
    pub async fn queued_write_rows_for_test(&self) -> Result<Vec<Vec<String>>, DbError> {
        Ok(self
            .inner
            .handle
            .test_queued_writes()
            .await?
            .into_iter()
            .map(|write| {
                let mut tables: Vec<String> = write
                    .parts
                    .iter()
                    .flat_map(|part| {
                        part.rows.iter().map(|change| change.row.table.clone()).chain(
                            part.executions
                                .iter()
                                .map(|row| format!("{} (unchanged)", row.row.table)),
                        )
                    })
                    .collect();
                tables.sort();
                tables
            })
            .collect())
    }

    /// How many writes this device committed and has not yet sent to storage.
    pub async fn unsent_writes_for_test(&self) -> Result<u64, DbError> {
        Ok(self.inner.handle.unsent_work().await.map_err(DbError::from)?.writes)
    }

    /// How many files wait in this device's upload queue.
    pub async fn queued_upload_count_for_test(&self) -> Result<i64, DbError> {
        self.read(|sql| {
            Ok(sql
                .sql
                .query_row("SELECT COUNT(*) FROM coven_upload_queue", [], |row| row.get(0))?)
        })
        .await
    }

    /// How many of the files waiting in this device's upload queue are in
    /// `namespace`.
    pub async fn queued_upload_count_in_namespace_for_test(
        &self,
        namespace: &str,
    ) -> Result<i64, DbError> {
        let namespace = namespace.to_string();
        self.read(move |sql| {
            Ok(sql.sql.query_row(
                "SELECT COUNT(*) FROM coven_uploads AS upload \
                 JOIN coven_files AS f ON f.id = upload.file WHERE f.namespace = ?1",
                [namespace],
                |row| row.get(0),
            )?)
        })
        .await
    }
}
