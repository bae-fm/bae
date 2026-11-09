/// Why the library refused a change this device asked for, once sync had
/// caught up with it (§5.4, §5.6). A target that is gone is named by its
/// kind; the change it was part of says which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOperationRejection {
    /// The account that owns the library's storage can't be removed.
    StoreOwner,
    PersonGone,
    DeviceGone,
    CircleGone,
    /// The library would have no admin left.
    NoAdminLeft,
    /// The keys it would bind are already bound.
    KeysAlreadyUsed,
    /// Another change replaced the person's keys first.
    MemberKeysChanged,
    /// This device, or its person's role, doesn't allow the change.
    NotAllowed,
    /// An approval would send the joining device a copy of every key the
    /// library has had, more than one admission holds.
    AdmissionTooLarge,
}
impl BridgeOperationRejection {
    pub(crate) fn from_core(value: coven::OperationRejection) -> Self {
        match value {
            coven::OperationRejection::StoreOwner => Self::StoreOwner,
            coven::OperationRejection::TargetGone(coven::EntryTarget::Member(_)) => {
                Self::PersonGone
            }
            coven::OperationRejection::TargetGone(coven::EntryTarget::Device(_)) => {
                Self::DeviceGone
            }
            coven::OperationRejection::TargetGone(coven::EntryTarget::Circle(_)) => {
                Self::CircleGone
            }
            coven::OperationRejection::NoAdminLeft => Self::NoAdminLeft,
            coven::OperationRejection::KeysAlreadyUsed => Self::KeysAlreadyUsed,
            coven::OperationRejection::MemberKeysChanged(_) => Self::MemberKeysChanged,
            coven::OperationRejection::NotAllowed => Self::NotAllowed,
            coven::OperationRejection::AdmissionTooLarge => Self::AdmissionTooLarge,
        }
    }
}
