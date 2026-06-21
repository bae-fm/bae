/// Public-key display helpers for the membership UI, shared by the onboarding
/// join flow and the Members settings tab.
enum MemberFormat {
    /// First eight hex characters of a device's public key — enough to tell
    /// devices apart at a glance without showing the full key.
    static func fingerprint(_ pubkey: String) -> String {
        String(pubkey.prefix(8))
    }
}
