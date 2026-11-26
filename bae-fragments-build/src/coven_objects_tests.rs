use super::coven_objects;
use sqlive_reads::{SchemaProbe, StandInFunction};

/// Every object app SQL may read prepares on the schema the build checks
/// against, so no function the objects call lacks a stand-in.
#[test]
fn each_readable_object_prepares_with_the_stand_in_functions() {
    let objects = coven_objects();
    let functions: Vec<StandInFunction> = objects
        .functions
        .iter()
        .map(|function| StandInFunction {
            name: function.name.clone(),
            arguments: function.arguments,
            deterministic: function.deterministic,
        })
        .collect();
    let probe = SchemaProbe::open(&[objects.sql], &functions, &[]).unwrap();
    for name in coven_schema::READABLE {
        probe
            .prepare(&format!("SELECT * FROM {name}"))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn a_function_is_a_coven_name_followed_by_an_opening_parenthesis() {
    let names = super::functions_named_in(
        "CREATE VIEW coven_a AS SELECT _coven_device(), xcoven_b(1), coven_c (2), coven_d(3)",
    );
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(names, ["_coven_device", "coven_d"]);
}
