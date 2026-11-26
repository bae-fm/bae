//! The people domain's rows that other reads join to. The columns are read by
//! the joins' SQL, and no Rust code reads them.

// A person's profile, which keeps its member id after the member's removal.
sqlive_compose::fragment!(
    Profile { id: coven::MemberId },
    r#"
    SELECT p.id, p.name
    FROM people AS p
"#
);
