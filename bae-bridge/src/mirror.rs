//! Macros for the mirrored halves of a bridge type's conversions.
//!
//! A `Bridge*` record or enum restates a bae-core type field for field so
//! uniffi has a concrete type to export, and the conversion between the two is
//! a name-for-name copy with the nested conversions applied. The copy is what
//! these macros write; the shapes themselves stay declared by hand, so a
//! variant or field added in bae-core still fails to compile here.
//!
//! The generated bodies destructure exhaustively — no `..` in any struct or
//! enum-variant pattern — which is what makes a new bae-core field a build
//! error rather than a field that silently never crosses.
//!
//! A conversion that renames, derives, or drops anything is not a copy and
//! stays written out.
//!
//! # Field and payload converters
//!
//! A name on its own moves the value. A parenthesized converter names the
//! bridge type on the other side of a nested conversion:
//!
//! - `pressing: (BridgePressingEdit)` — the value converts on its own
//! - `tracks: (each BridgeTrackUserEdit)` — every element of a collection
//!   converts; the container is whatever the two sides declare, which is a
//!   `Vec` on most fields and a set on some
//! - `cover: (opt BridgeImageRef)` — an `Option` of one

/// One field or payload crossing outward, with its nested conversion applied.
macro_rules! mirror_from {
    ($value:expr) => {
        $value
    };
    ($value:expr, (each $bridge:path)) => {
        $value.into_iter().map(<$bridge>::from_core).collect()
    };
    ($value:expr, (opt $bridge:path)) => {
        $value.map(<$bridge>::from_core)
    };
    ($value:expr, ($bridge:path)) => {
        <$bridge>::from_core($value)
    };
}

/// One field or payload crossing inward, with its nested conversion applied.
macro_rules! mirror_into {
    ($value:expr) => {
        $value
    };
    ($value:expr, (each $bridge:path)) => {
        $value.into_iter().map(<$bridge>::into_core).collect()
    };
    ($value:expr, (opt $bridge:path)) => {
        $value.map(<$bridge>::into_core)
    };
    ($value:expr, ($bridge:path)) => {
        $value.into_core()
    };
}

/// The field-for-field copy between a `Bridge*` record and its bae-core struct.
///
/// ```ignore
/// mirror_struct! {
///     #[cfg(feature = "desktop")]
///     BridgeReleaseUserEdit = bae_core::import::ReleaseUserEdit,
///     from_core: pub(crate) fn,
///     into_core: pub(crate) fn,
///     fields: {
///         album_title,
///         album_artist_assignments: (each BridgeArtistAssignment),
///         pressing: (BridgePressingEdit),
///     },
/// }
/// ```
///
/// Either direction may stand alone; each carries its own visibility and, if it
/// needs one, its own attributes. Asking for both writes one impl block per
/// direction.
macro_rules! mirror_struct {
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$from_attr:meta])*
        from_core: $from_vis:vis fn,
        $(#[$into_attr:meta])*
        into_core: $into_vis:vis fn,
        fields: { $($fields:tt)* } $(,)?
    ) => {
        mirror_struct! {
            $(#[$attr])*
            $bridge = $core,
            $(#[$from_attr])*
            from_core: $from_vis fn,
            fields: { $($fields)* }
        }
        mirror_struct! {
            $(#[$attr])*
            $bridge = $core,
            $(#[$into_attr])*
            into_core: $into_vis fn,
            fields: { $($fields)* }
        }
    };
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$from_attr:meta])*
        from_core: $from_vis:vis fn,
        fields: { $($field:ident $(: $conv:tt)?),* $(,)? } $(,)?
    ) => {
        $(#[$attr])*
        impl $bridge {
            $(#[$from_attr])*
            $from_vis fn from_core(value: $core) -> Self {
                type Core = $core;
                let Core { $($field),* } = value;
                Self { $($field: mirror_from!($field $(, $conv)?)),* }
            }
        }
    };
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$into_attr:meta])*
        into_core: $into_vis:vis fn,
        fields: { $($field:ident $(: $conv:tt)?),* $(,)? } $(,)?
    ) => {
        $(#[$attr])*
        impl $bridge {
            $(#[$into_attr])*
            $into_vis fn into_core(self) -> $core {
                type Core = $core;
                let Self { $($field),* } = self;
                Core { $($field: mirror_into!($field $(, $conv)?)),* }
            }
        }
    };
}

/// The variant-for-variant copy between a `Bridge*` enum and its bae-core enum.
///
/// ```ignore
/// mirror_enum! {
///     #[cfg(feature = "desktop")]
///     BridgeSheetBound = bae_core::import::SheetBound,
///     from_core: fn,
///     into_core: fn,
///     variants: {
///         Describes(container: (BridgeMappingContainer)),
///         DescribesFiles,
///         Unresolved { requested },
///     },
/// }
/// ```
///
/// A variant is written as bae-core spells it: a bare name for a unit variant,
/// `Name { field, … }` for a struct variant, and `Name(field: …)` where
/// bae-core carries one unnamed payload — uniffi has no tuple variants, so the
/// bridge side names it.
macro_rules! mirror_enum {
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$from_attr:meta])*
        from_core: $from_vis:vis fn,
        $(#[$into_attr:meta])*
        into_core: $into_vis:vis fn,
        variants: { $($variants:tt)* } $(,)?
    ) => {
        mirror_enum! {
            $(#[$attr])*
            $bridge = $core,
            $(#[$from_attr])*
            from_core: $from_vis fn,
            variants: { $($variants)* }
        }
        mirror_enum! {
            $(#[$attr])*
            $bridge = $core,
            $(#[$into_attr])*
            into_core: $into_vis fn,
            variants: { $($variants)* }
        }
    };
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$from_attr:meta])*
        from_core: $from_vis:vis fn,
        variants: {
            $(
                $variant:ident
                $(( $payload:ident $(: $payload_conv:tt)? ))?
                $({ $($field:ident $(: $conv:tt)?),* $(,)? })?
            ),* $(,)?
        } $(,)?
    ) => {
        $(#[$attr])*
        impl $bridge {
            $(#[$from_attr])*
            $from_vis fn from_core(value: $core) -> Self {
                type Core = $core;
                match value {
                    $(
                        Core::$variant
                            $(( $payload ))?
                            $({ $($field),* })?
                        => Self::$variant
                            $({ $payload: mirror_from!($payload $(, $payload_conv)?) })?
                            $({ $($field: mirror_from!($field $(, $conv)?)),* })?,
                    )*
                }
            }
        }
    };
    (
        $(#[$attr:meta])*
        $bridge:ty = $core:ty,
        $(#[$into_attr:meta])*
        into_core: $into_vis:vis fn,
        variants: {
            $(
                $variant:ident
                $(( $payload:ident $(: $payload_conv:tt)? ))?
                $({ $($field:ident $(: $conv:tt)?),* $(,)? })?
            ),* $(,)?
        } $(,)?
    ) => {
        $(#[$attr])*
        impl $bridge {
            $(#[$into_attr])*
            $into_vis fn into_core(self) -> $core {
                type Core = $core;
                match self {
                    $(
                        Self::$variant
                            $({ $payload })?
                            $({ $($field),* })?
                        => Core::$variant
                            $(( mirror_into!($payload $(, $payload_conv)?) ))?
                            $({ $($field: mirror_into!($field $(, $conv)?)),* })?,
                    )*
                }
            }
        }
    };
}
