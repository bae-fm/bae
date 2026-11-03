//! People's open invites, as People lists them.

use super::*;

impl LibraryViews {
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one, live from coven's `coven_invites` and
    /// `coven_pending` in one read.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<bae_db::people::OpenInvite>> {
        self.database.subscribe(|sql| {
            let records = sql.query("SELECT record, cause FROM coven_pending", [], |row| {
                let mut record = row.get::<_, coven::PendingRecord>(0)?;
                record.cause = row.get(1)?;
                Ok(record)
            })?;
            Ok(bae_db::people::OpenInvite::held_by(coven::Invite::list(sql)?, &records))
        })
    }
}
