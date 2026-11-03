use super::*;

#[test]
fn internal_ui_diagnostics_remain_internal() {
    let error = AutomationError::from(bae_diagnostics::UiError::internal("admission invariant"));
    assert!(matches!(
        error,
        AutomationError::Internal(detail) if detail == "admission invariant"
    ));
}
