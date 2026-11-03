/// What an import candidate is linked to in the catalogs: the pressing a
/// person picked, or identification picked unattended, for its folder; or,
/// where the person could not tell which of the pressings offered their copy
/// is, the album those pressings are of.
///
/// Independent of the draft. Only explicit actions set or clear it — a pick,
/// identification applying its pick, linking the album, keeping the
/// folder's own draft over the lookup, unlinking — so editing, clearing, or
/// re-reading the draft leaves it standing. Importing takes the draft's
/// values and this link's catalog identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseLink {
    Pressing(bae_identify::PressingLink),
    Album(bae_identify::AlbumLink),
}

impl ReleaseLink {
    /// The pressing the link names, where it names one.
    pub fn pressing(&self) -> Option<&bae_identify::PressingLink> {
        match self {
            Self::Pressing(pressing) => Some(pressing),
            Self::Album(_) => None,
        }
    }
}
