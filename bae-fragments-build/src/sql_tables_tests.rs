use super::tables_read;

fn read(sql: &str) -> Vec<String> {
    tables_read(sql).into_iter().collect()
}

#[test]
fn a_table_after_from_and_each_join_is_read() {
    assert_eq!(
        read("SELECT a.id FROM albums AS a JOIN tracks t ON t.album_id = a.id LEFT JOIN covers c ON 1"),
        ["albums", "covers", "tracks"]
    );
}

#[test]
fn a_comma_between_from_items_names_another_table() {
    assert_eq!(
        read("SELECT 1 FROM albums a, tracks t WHERE a.id = t.album_id"),
        ["albums", "tracks"]
    );
}

#[test]
fn a_comma_in_a_select_list_or_after_where_names_no_table() {
    assert_eq!(
        read("SELECT a.id, a.title FROM albums a WHERE a.id IN (1, 2) GROUP BY a.id, a.title ORDER BY a.id, a.title"),
        ["albums"]
    );
}

#[test]
fn a_subquery_and_a_union_name_the_tables_they_read() {
    assert_eq!(
        read("SELECT x.id FROM (SELECT id FROM albums UNION ALL SELECT id FROM works) AS x"),
        ["albums", "works"]
    );
}

#[test]
fn a_common_table_expression_and_a_function_are_no_tables() {
    assert_eq!(
        read("WITH chosen (id) AS (SELECT id FROM albums), other AS (SELECT 1) SELECT * FROM chosen, other, json_each(:ids)"),
        ["albums"]
    );
}

#[test]
fn a_spread_a_comment_and_a_string_are_no_tables() {
    assert_eq!(
        read("SELECT 'from tracks' -- from works\n FROM @records::Album AS a /* join covers */ JOIN @records::Track AS t ON 1"),
        Vec::<String>::new()
    );
}

#[test]
fn a_schema_prefix_and_a_quoted_name_are_read_as_the_table() {
    assert_eq!(read("SELECT 1 FROM main.Albums JOIN \"tracks\" ON 1"), ["albums", "tracks"]);
}

#[test]
fn a_table_in_an_exists_subquery_of_a_condition_is_read() {
    assert_eq!(
        read(
            "SELECT 1 FROM albums a WHERE EXISTS (SELECT 1 FROM tracks t WHERE t.album_id = a.id)"
        ),
        ["albums", "tracks"]
    );
}
