use super::foreign_reads;
use crate::domains::Domains;
use sqlive_compose::compiler::{compile_crate, Input, Setup};

const POLICY: &str = r#"
[database]
capability = "SQLite"

[[database.domains]]
name = "records"
owner = "bae_records::Records"
homes = ["bae-music/src/records.rs"]
tables = ["probe_books"]

[[database.domains]]
name = "people"
owner = "bae_people::People"
homes = ["bae-music/src/people.rs"]
tables = ["probe_reviews"]
"#;

const SCHEMA: &str = "
    CREATE TABLE probe_books (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL) STRICT;
    CREATE TABLE probe_reviews (book_id TEXT PRIMARY KEY NOT NULL, stars INTEGER NOT NULL) STRICT;
";

const RECORDS: &str = r##"
coven::fragment!(
    BookTitle(id: String),
    r#"SELECT book.title AS title FROM probe_books AS book WHERE book.id = :id"#
);
"##;

/// The refusals of the crate `bae-music`, whose records domain holds
/// [`RECORDS`] and whose people domain holds the source `people`.
fn refusals(people: &str) -> Vec<String> {
    let compiled = compile_crate(Input {
        crate_name: "bae-music".to_owned(),
        macros_crate: "coven".to_owned(),
        source_reader: &|path| match path {
            "src/lib.rs" => Ok("mod people;\nmod records;\n".to_owned()),
            "src/people.rs" => Ok(people.to_owned()),
            "src/records.rs" => Ok(RECORDS.to_owned()),
            _ => Err(std::io::ErrorKind::NotFound.into()),
        },
        manifests: Vec::new(),
        setup: Setup {
            sql: vec![SCHEMA.to_owned()],
            functions: Vec::new(),
            collations: Vec::new(),
            writer_only: Vec::new(),
        },
    })
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    foreign_reads("bae-music", &Domains::from_policy(POLICY).unwrap(), &compiled.calls)
}

#[test]
fn a_read_that_names_another_domains_table_is_refused_with_the_domain() {
    let refused = refusals(
        r##"coven::read!(
    Rated(id: String),
    r#"SELECT review.stars AS stars, book.title AS title FROM probe_reviews AS review
       JOIN probe_books AS book ON book.id = review.book_id WHERE review.book_id = :id"#
);
"##,
    );
    assert_eq!(
        refused,
        ["bae-music/src/people.rs:1: read! Rated reads probe_books, a table of the domain \"records\", \
          whose homes don't hold this file; read it through a fragment of that domain"]
    );
}

#[test]
fn a_write_that_reads_another_domains_table_is_refused_with_the_domain() {
    let refused = refusals(
        r##"coven::write!(
    RateEveryBook(stars: i64),
    r#"INSERT INTO probe_reviews (book_id, stars) SELECT book.id, :stars FROM probe_books AS book"#
);
"##,
    );
    assert_eq!(
        refused,
        ["bae-music/src/people.rs:1: write! RateEveryBook reads probe_books, a table of the domain \
          \"records\", whose homes don't hold this file; read it through a fragment of that domain"]
    );
}

#[test]
fn a_read_that_spreads_another_domains_fragment_passes() {
    let refused = refusals(
        r##"coven::read!(
    Rated(id: String),
    r#"SELECT review.stars AS stars, book.* FROM probe_reviews AS review
       JOIN @crate::records::BookTitle(id: :id) AS book ON 1 WHERE review.book_id = :id"#
);
"##,
    );
    assert_eq!(refused, Vec::<String>::new());
}
