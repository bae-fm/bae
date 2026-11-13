//! The device's sort language through the bridge object the platforms hold.

/// The device's sort language is set through the handle: a language tag is
/// taken, a repeat writes nothing, and something that is not one is refused
/// as a configuration failure.
#[test]
fn the_sort_language_is_set_through_the_handle() {
    use crate::types::{BridgeError, BridgeErrorCategory};
    let handle = super::super::tests::fresh_bridge_handle("sort-language");
    let run = |tag: &str| {
        handle.handle().runtime.block_on(handle.handle().clone().set_sort_language(tag.to_string()))
    };

    run("sv").expect("a language tag is taken");
    run("sv").expect("the language it already has is taken again");
    assert!(matches!(
        run("not a language"),
        Err(BridgeError::Diagnostic { category: BridgeErrorCategory::Config, .. })
    ));
}
