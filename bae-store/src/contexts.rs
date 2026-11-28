//! The contexts bae's reads and writes run in. Each one holds one of coven's
//! and runs only the statements bae's build generated (`read!`, `write!`),
//! through [`WriterRead`](crate::WriterRead) and
//! [`WriteStatements`](crate::WriteStatements); a write's context also runs
//! coven's own steps, each by its name. No context takes SQL text, so no
//! code outside bae-store can run a statement its build did not compile.

/// A read's context: a one-shot read's, on one of coven's readers.
pub struct ReadContext<'c> {
    pub(crate) sql: coven::SqlReadContext<'c>,
}

impl<'c> ReadContext<'c> {
    pub(crate) fn new(sql: coven::SqlReadContext<'c>) -> Self {
        Self { sql }
    }

    /// What `reads`, statements bae's build compiled, return, read once in
    /// this read's snapshot.
    pub fn reads<S: coven::LiveReads>(&self, reads: &S) -> Result<S::Rows, coven::DbError> {
        self.sql.reads(reads)
    }
}

/// A write's context: one transaction on the store's writer, which runs the
/// write's statements and coven's steps and commits them together.
pub struct WriteContext<'w> {
    pub(crate) sql: coven::CovenWrite<'w>,
}

/// coven's steps that bae's writes take, each run in the write as coven's
/// `CovenWrite` runs it: its effect commits or rolls back with the write's
/// statements.
impl<'w> WriteContext<'w> {
    pub(crate) fn new(sql: coven::CovenWrite<'w>) -> Self {
        Self { sql }
    }

    /// Stage the read original `original` as a new file of `namespace`, on
    /// this device only or for the cloud, and return its id.
    pub fn put_user_file(
        &self,
        namespace: &str,
        original: coven::PreparedUserFile,
        place: coven::Place,
    ) -> Result<coven::FileId, coven::DbError> {
        self.sql.put_user_file(namespace, original, place)
    }

    /// Record a move of `files` to the cloud.
    pub fn move_to_cloud(
        &self,
        files: Vec<coven::MoveFile>,
    ) -> Result<coven::MoveId, coven::DbError> {
        self.sql.move_to_cloud(files)
    }

    /// Cancel the move `id`: its files stay on this device only.
    pub fn cancel_move(&self, id: coven::MoveId) -> Result<(), coven::DbError> {
        self.sql.cancel_move(id)
    }

    /// Remove `files`' copies from this device once the write commits.
    pub fn unpin(&self, files: &[coven::FileId]) -> Result<(), coven::DbError> {
        self.sql.unpin(files)
    }

    /// Clear `namespace`'s cache of every unpinned file once the write
    /// commits.
    pub fn clear_cache(&self, namespace: &str) -> Result<(), coven::DbError> {
        self.sql.clear_cache(namespace)
    }

    /// Set `namespace`'s cache budget to `max_bytes`, evicting what is past
    /// it once the write commits.
    pub fn set_cache_budget(&self, namespace: &str, max_bytes: u64) -> Result<(), coven::DbError> {
        self.sql.set_cache_budget(namespace, max_bytes)
    }

    /// Keep the files of the prepared pin `prepared` whole on this device.
    pub fn pin(&self, prepared: coven::PreparedPin) -> Result<(), coven::DbError> {
        self.sql.pin(prepared)
    }

    /// Switch the files of `prepared` to the copies made on this device.
    pub fn make_device_only(
        &self,
        prepared: coven::PreparedDeviceOnly,
    ) -> Result<(), coven::DbError> {
        self.sql.make_device_only(prepared)
    }

    /// Rename the library, as an admin.
    pub fn rename_store(&self, name: &str) -> Result<Option<coven::OperationId>, coven::SyncError> {
        self.sql.rename_store(name)
    }

    /// Give `member` the role `role`, as an admin.
    pub fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<Option<coven::OperationId>, coven::SyncError> {
        self.sql.set_member_role(member, role)
    }

    /// Remove `member` and all their devices, as an admin.
    pub fn remove_member(
        &self,
        member: &coven::MemberId,
    ) -> Result<Option<coven::OperationId>, coven::SyncError> {
        self.sql.remove_member(member)
    }

    /// Remove `device`, and say how its member signs in again on the
    /// devices they keep.
    pub fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<(Option<coven::OperationId>, Option<coven::ProviderSignOut>), coven::SyncError>
    {
        self.sql.remove_device(device)
    }

    /// Approve the join request `request`.
    pub fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<coven::OperationId, coven::SyncError> {
        self.sql.approve_join_request(request)
    }

    /// Acknowledge the finished operation `operation`'s outcome.
    pub fn acknowledge_operation(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), coven::DbError> {
        self.sql.acknowledge_operation(operation)
    }
}

/// A bulk write's context: statements that change device-local tables alone,
/// run without coven's pre-update hook.
pub struct BulkContext<'c> {
    pub(crate) sql: &'c coven::BulkSqlContext<'c>,
}

/// The context of a domain's function that coven runs last in each
/// transaction that changes the app's rows.
pub struct CommitContext<'c> {
    pub(crate) sql: &'c coven::CommitSql<'c>,
}

/// The context of a domain's write for a member's device as its
/// registration applies.
pub struct JoiningContext<'c> {
    pub(crate) sql: &'c coven::JoiningWrite<'c>,
}

/// The context of a domain's write in the library's first write.
pub struct MemberContext<'c> {
    pub(crate) sql: &'c coven::MemberWrite<'c>,
}
