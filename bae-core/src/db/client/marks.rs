//! Reading and writing a release's marks — the names read off the object
//! itself.

use super::*;

impl Database {
    /// Insert one row per sighting for an existing release. Used for writing
    /// marks outside of the atomic import path.
    pub async fn insert_release_marks(
        &self,
        release_id: &str,
        marks: &[crate::import::ReleaseMark],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let marks = marks.to_vec();
        let now = self.inner.clock.now().to_rfc3339();
        let ids = Arc::clone(&self.inner.ids);
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            for (position, mark) in marks.iter().enumerate() {
                insert_release_mark_row(
                    &sql,
                    &release_id,
                    mark,
                    position,
                    ids.new_id(),
                    &reg,
                    &now,
                )?;
            }
            Ok(())
        })
        .await
    }

    /// Every mark sighting a release carries, in the order they were read.
    /// Empty for a release whose folder stated no name at all.
    pub async fn get_release_marks(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseMark>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_release_marks_on(&sql, &release_id))
            .await
    }
}
