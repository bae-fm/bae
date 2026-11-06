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

    /// How many rows of `table` have `column` equal to `value`.
    pub async fn count_rows_for_test(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<i64, DbError> {
        let query = format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" = ?");
        let value = value.to_string();
        self.read(move |sql| {
            sql.query_row(&query, rusqlite::params![value], |row| row.get(0)).map_err(DbError::from)
        })
        .await
    }

    pub fn store_id_for_test(&self) -> coven::StoreId {
        self.store_id()
    }

    /// Every row of every table bae can read, rendered, by table name: what
    /// a test compares to show an operation left the store as it found it.
    /// Coven's own tables are closed to bae's connections and are left out;
    /// the files coven keeps for bae are files under the library directory.
    pub async fn every_row_for_test(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<String>>, DbError> {
        self.read(|sql| {
            let tables: Vec<String> = sql.query(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND substr(name, 1, 7) <> '_coven_' AND substr(name, 1, 7) <> 'sqlite_' \
                 ORDER BY name",
                [],
                |row| row.get(0),
            )?;
            let mut every_row = std::collections::BTreeMap::new();
            for table in tables {
                let mut rows = sql.query(&format!("SELECT * FROM \"{table}\""), [], |row| {
                    (0..row.as_ref().column_count())
                        .map(|column| {
                            row.get_ref(column).map(|value| match value {
                                rusqlite::types::ValueRef::Text(text) => {
                                    String::from_utf8_lossy(text).into_owned()
                                }
                                other => format!("{other:?}"),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|values| values.join(" | "))
                })?;
                rows.sort();
                every_row.insert(table, rows);
            }
            Ok(every_row)
        })
        .await
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
            Ok(sql.query_row("SELECT COUNT(*) FROM coven_upload_queue", [], |row| row.get(0))?)
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
            Ok(sql.query_row(
                "SELECT COUNT(*) FROM coven_uploads AS upload \
                 JOIN coven_files AS f ON f.id = upload.file WHERE f.namespace = ?1",
                [namespace],
                |row| row.get(0),
            )?)
        })
        .await
    }

    /// Answer one text-valued query against this device's library.
    pub async fn query_texts_for_test(&self, query: &str) -> Result<Vec<String>, DbError> {
        let query = query.to_string();
        self.read(move |sql| {
            sql.query(&query, [], |row| row.get::<_, String>(0)).map_err(DbError::from)
        })
        .await
    }
}
