//! Re-export of coven's encryption service. The encryption primitives live in
//! coven now; this keeps bae's `crate::encryption::…` call sites resolving
//! unchanged. bae's old `derive_release_encryption(id)` is coven's generic
//! `derive_scoped(id)` — call sites use the new name.
pub use coven::encryption::*;
