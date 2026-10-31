//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one) or from a
//! `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.

use std::sync::Arc;

use crate::types::BridgeError;

/// A live query's values as the app pulls them: `$object` holds core's
/// [`LiveValues`](bae_core::library::LiveValues) of `$core`, each pulled
/// value converted to `$bridge`, and `$make` builds it.
macro_rules! pulled_values {
    (
        $(#[doc = $doc:literal])*
        $object:ident: $core:ty => $bridge:ty, built by $make:ident
    ) => {
        $(#[doc = $doc])*
        #[derive(uniffi::Object)]
        pub struct $object {
            values: bae_core::library::LiveValues<$core>,
            runtime: crate::operation_runtime::OperationRuntime,
        }

        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
            /// The first value at once, then one after each write that
            /// changes it. A failed read is answered as an error; the next
            /// pull waits for the read after it.
            pub async fn next(self: Arc<Self>) -> Result<$bridge, BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".next"), move || async move {
                        self.values
                            .next()
                            .await
                            .map(<$bridge>::from_core)
                            .map_err(values_error)
                    })
                    .await
            }

            /// End the subscription: a waiting pull answers that it was
            /// cancelled, and so does every later one.
            pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".cancel"), move || async move {
                        self.values.cancel().await;
                        Ok(())
                    })
                    .await
            }
        }

        pub(crate) fn $make(
            values: bae_core::library::LiveValues<$core>,
            runtime: crate::operation_runtime::OperationRuntime,
        ) -> Arc<$object> {
            Arc::new($object { values, runtime })
        }
    };
}

fn values_error(error: bae_core::library::LiveValuesError) -> BridgeError {
    match error {
        bae_core::library::LiveValuesError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LiveValuesError::Query(error) => BridgeError::database_query(error),
    }
}

pulled_values! {
    /// How far the identifications and imports in flight have got.
    ImportCountsSubscription: bae_core::import::ImportCounts => crate::types::BridgeImportCounts,
    built by import_counts
}
