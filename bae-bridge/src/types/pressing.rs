//! What a pressing is, as a surface renders it: the typed facts bae-core reads
//! out of both catalogs (`bae_core::pressing`), and the words each is shown
//! in. A surface never sees a catalog's raw text — a country crosses as its
//! ISO 3166-1 code, which the platform names in the reader's language; every
//! other fact crosses typed, and the functions here say which catalog
//! message or printed term names it.

/// Where a pressing was released. Mirrors `bae_core::pressing::ReleaseArea`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeReleaseArea {
    /// A country of ISO 3166-1, by its alpha-2 code. The platform names it.
    Country {
        code: String,
    },
    Region {
        region: BridgeRegion,
    },
}

impl BridgeReleaseArea {
    pub(crate) fn from_core(area: bae_core::pressing::ReleaseArea) -> Self {
        match area {
            bae_core::pressing::ReleaseArea::Country(country) => Self::Country {
                code: country.code().to_string(),
            },
            bae_core::pressing::ReleaseArea::Region(region) => Self::Region {
                region: BridgeRegion::from_core(region),
            },
        }
    }

    /// The core area this names. A code that names no country of the
    /// standard is not one a surface can have been offered.
    #[cfg(feature = "desktop")]
    pub(crate) fn into_core(self) -> bae_core::pressing::ReleaseArea {
        match self {
            Self::Country { code } => bae_core::pressing::ReleaseArea::Country(
                bae_core::pressing::Country::from_code(&code).unwrap_or_else(|| {
                    panic!("{code:?} is no ISO 3166-1 country the bridge offered")
                }),
            ),
            Self::Region { region } => bae_core::pressing::ReleaseArea::Region(region.into_core()),
        }
    }
}

/// How many of one carrier a pressing holds. Mirrors
/// `bae_core::pressing::MediaCount`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMediaCount {
    pub medium: BridgeMedium,
    pub count: u32,
}

mirror_struct! {
    BridgeMediaCount = bae_core::pressing::MediaCount,
    from_core: pub(crate) fn,
    #[cfg(feature = "desktop")]
    into_core: pub(crate) fn,
    fields: { medium: (BridgeMedium), count },
}

/// What a pressing is. Mirrors `bae_core::pressing::PressingFacts`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePressingFacts {
    pub area: Option<BridgeReleaseArea>,
    pub media: Vec<BridgeMediaCount>,
    pub status: Option<BridgeReleaseStatus>,
    pub packaging: Option<BridgePackaging>,
    pub discogs_details: Vec<BridgeDiscogsDetail>,
}

mirror_struct! {
    BridgePressingFacts = bae_core::pressing::PressingFacts,
    from_core: pub(crate) fn,
    #[cfg(feature = "desktop")]
    into_core: pub(crate) fn,
    fields: {
        area: (opt BridgeReleaseArea),
        media: (each BridgeMediaCount),
        status: (opt BridgeReleaseStatus),
        packaging: (opt BridgePackaging),
        discogs_details: (each BridgeDiscogsDetail),
    },
}

/// A carrier family. Mirrors `bae_core::pressing::Medium`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeMedium {
    Cd,
    Sacd,
    Dvd,
    HdDvd,
    BluRay,
    Vinyl,
    Shellac,
    Acetate,
    Cassette,
    Dcc,
    Dat,
    Cartridge,
    ReelToReel,
    Digital,
    MiniDisc,
    LaserDisc,
    Cdv,
    VideoCd,
    VideoTape,
    FlashMemory,
    Floppy,
    Cylinder,
    DualDisc,
    VinylDisc,
    DvdPlus,
    CdRecord,
    PianoRoll,
    EdisonDisc,
    PatheDisc,
    Tefifon,
    Umd,
    Vhd,
    SelectaVision,
    Ted,
    Mvd,
    WireRecording,
    FilmReel,
    MightyTiny,
    Sopic,
    KitAlbum,
    RomCartridge,
    DataPlay,
}

mirror_enum! {
    BridgeMedium = bae_core::pressing::Medium,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Cd, Sacd, Dvd, HdDvd, BluRay, Vinyl,
        Shellac, Acetate, Cassette, Dcc, Dat, Cartridge,
        ReelToReel, Digital, MiniDisc, LaserDisc, Cdv, VideoCd,
        VideoTape, FlashMemory, Floppy, Cylinder, DualDisc, VinylDisc,
        DvdPlus, CdRecord, PianoRoll, EdisonDisc, PatheDisc, Tefifon,
        Umd, Vhd, SelectaVision, Ted, Mvd, WireRecording,
        FilmReel, MightyTiny, Sopic, KitAlbum, RomCartridge, DataPlay,
    },
}

/// An area no current ISO 3166-1 code names. Mirrors `bae_core::pressing::Region`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeRegion {
    Europe,
    Worldwide,
    Africa,
    Asia,
    MiddleEast,
    SouthEastAsia,
    CentralAmerica,
    SouthAmerica,
    NorthAmerica,
    NorthAndSouthAmerica,
    Australasia,
    SouthPacific,
    Scandinavia,
    Benelux,
    GulfCooperationCouncil,
    UkAndEurope,
    UkAndIreland,
    UkAndUs,
    UkAndFrance,
    UkAndGermany,
    UkEuropeAndUs,
    UkEuropeAndJapan,
    UkEuropeAndIsrael,
    UsaAndCanada,
    UsaAndEurope,
    UsaCanadaAndEurope,
    UsaCanadaAndUk,
    GermanyAndSwitzerland,
    GermanyAustriaAndSwitzerland,
    FranceAndBenelux,
    CzechRepublicAndSlovakia,
    RussiaAndCis,
    AustraliaAndNewZealand,
    SingaporeAndMalaysia,
    SingaporeMalaysiaAndHongKong,
    SingaporeMalaysiaHongKongAndThailand,
    HongKongAndThailand,
    SovietUnion,
    Yugoslavia,
    Czechoslovakia,
    EastGermany,
    SerbiaAndMontenegro,
    NetherlandsAntilles,
    Kosovo,
    Abkhazia,
    AustriaHungary,
    OttomanEmpire,
    Bohemia,
    ProtectorateOfBohemiaAndMoravia,
    KoreaBefore1945,
    SouthVietnam,
    Indochina,
    DutchEastIndies,
    BelgianCongo,
    Zaire,
    Rhodesia,
    SouthernRhodesia,
    SouthWestAfrica,
    Dahomey,
    UpperVolta,
    Zanzibar,
    ItalianEastAfrica,
}

mirror_enum! {
    BridgeRegion = bae_core::pressing::Region,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Europe, Worldwide, Africa, Asia, MiddleEast, SouthEastAsia,
        CentralAmerica, SouthAmerica, NorthAmerica, NorthAndSouthAmerica, Australasia, SouthPacific,
        Scandinavia, Benelux, GulfCooperationCouncil, UkAndEurope, UkAndIreland, UkAndUs,
        UkAndFrance, UkAndGermany, UkEuropeAndUs, UkEuropeAndJapan, UkEuropeAndIsrael, UsaAndCanada,
        UsaAndEurope, UsaCanadaAndEurope, UsaCanadaAndUk, GermanyAndSwitzerland, GermanyAustriaAndSwitzerland, FranceAndBenelux,
        CzechRepublicAndSlovakia, RussiaAndCis, AustraliaAndNewZealand, SingaporeAndMalaysia, SingaporeMalaysiaAndHongKong, SingaporeMalaysiaHongKongAndThailand,
        HongKongAndThailand, SovietUnion, Yugoslavia, Czechoslovakia, EastGermany, SerbiaAndMontenegro,
        NetherlandsAntilles, Kosovo, Abkhazia, AustriaHungary, OttomanEmpire, Bohemia,
        ProtectorateOfBohemiaAndMoravia, KoreaBefore1945, SouthVietnam, Indochina, DutchEastIndies, BelgianCongo,
        Zaire, Rhodesia, SouthernRhodesia, SouthWestAfrica, Dahomey, UpperVolta,
        Zanzibar, ItalianEastAfrica,
    },
}

/// How official a release is. Mirrors `bae_core::pressing::ReleaseStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeReleaseStatus {
    Official,
    Promotion,
    Bootleg,
    PseudoRelease,
    Withdrawn,
    Expunged,
    Cancelled,
}

mirror_enum! {
    BridgeReleaseStatus = bae_core::pressing::ReleaseStatus,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Official, Promotion, Bootleg, PseudoRelease, Withdrawn, Expunged,
        Cancelled,
    },
}

/// What a release is sold in. Mirrors `bae_core::pressing::Packaging`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgePackaging {
    JewelCase,
    SlimJewelCase,
    Digipak,
    CardboardSleeve,
    Other,
    KeepCase,
    Unpackaged,
    CassetteCase,
    Book,
    Fatbox,
    SnapCase,
    GatefoldCover,
    DiscboxSlider,
    SuperJewelBox,
    Digibook,
    PlasticSleeve,
    Box,
    Slidepack,
    SnapPack,
    MetalTin,
    Longbox,
    ClamshellCase,
    Digifile,
    Slipcase,
}

mirror_enum! {
    BridgePackaging = bae_core::pressing::Packaging,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        JewelCase, SlimJewelCase, Digipak, CardboardSleeve, Other, KeepCase,
        Unpackaged, CassetteCase, Book, Fatbox, SnapCase, GatefoldCover,
        DiscboxSlider, SuperJewelBox, Digibook, PlasticSleeve, Box, Slidepack,
        SnapPack, MetalTin, Longbox, ClamshellCase, Digifile, Slipcase,
    },
}

/// A Discogs format description no field holds. Mirrors `bae_core::pressing::DiscogsDetail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeDiscogsDetail {
    Lp,
    Size16In,
    Size12In,
    Size14In,
    Size11In,
    Size10In,
    Size9In,
    Size8In,
    Size7In,
    Size6HalfIn,
    Size6In,
    Size5HalfIn,
    Size5In,
    Size4In,
    Size3HalfIn,
    Size3In,
    Size2In,
    Size1In,
    Rpm8Third,
    Rpm16TwoThirds,
    Rpm33Third,
    Rpm45,
    Rpm78,
    Rpm120,
    Size21Cm,
    Size25Cm,
    Size27Cm,
    Size29Cm,
    Size35Cm,
    Size40Cm,
    Size50Cm,
    Rpm80,
    Rpm90,
    IpsFifteenSixteenths,
    Ips1SevenEighths,
    Ips15,
    Ips3ThreeQuarters,
    Ips30,
    Ips7Half,
    HalfInchTape,
    QuarterInchTape,
    EighthInchTape,
    T2TrackMono,
    T2TrackStereo,
    T4TrackMono,
    T4TrackStereo,
    NabReel10Point5In,
    CineReel3In,
    CineReel5In,
    CineReel6In,
    CineReel7In,
    Minute2,
    Minute3,
    Minute4,
    Concert,
    Salon,
    Mini,
    BusinessCard,
    Shape,
    Minimax,
    CdRom,
    CdI,
    CdPlusG,
    Hdcd,
    Vcd,
    Avcd,
    Svcd,
    Xrcd,
    Uhd4k,
    Uhd8k,
    BluRayAudio,
    Multichannel,
    DvdAudio,
    DvdData,
    DvdVideo,
    Hybrid,
    Aac,
    Aifc,
    Aiff,
    Alac,
    Amr,
    Ape,
    Avi,
    Dff,
    DiscImage,
    Dsf,
    Flac,
    Flv,
    Mov,
    Mp1,
    Mp2,
    Mp3,
    MpegVideo,
    Mpeg4Video,
    OggVorbis,
    Opus,
    Ra,
    Rm,
    Shn,
    Spx,
    Swf,
    Tta,
    Wav,
    WavPack,
    Wma,
    Wmv,
    Mp3Surround,
    Floppy3Point5In,
    Floppy5Point25In,
    DoubleSided,
    SingleSided,
    Advance,
    RecordStoreDay,
    Stereo,
    Mono,
    Quadraphonic,
    Ambisonic,
    Bioplastic,
    CardBacked,
    ClubEdition,
    CopyProtected,
    DeluxeEdition,
    Enhanced,
    Etched,
    Jukebox,
    LimitedEdition,
    Mispress,
    Misprint,
    Numbered,
    PartiallyMixed,
    PartiallyUnofficial,
    PictureDisc,
    Reissue,
    Remastered,
    Repress,
    SpecialCut,
    SpecialEdition,
    Styrene,
    TestPressing,
    TourRecording,
    Transcription,
    WhiteLabel,
    Film16mm,
    Film35mm,
    Ntsc,
    Pal,
    Secam,
}

mirror_enum! {
    BridgeDiscogsDetail = bae_core::pressing::DiscogsDetail,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Lp, Size16In, Size12In, Size14In, Size11In, Size10In,
        Size9In, Size8In, Size7In, Size6HalfIn, Size6In, Size5HalfIn,
        Size5In, Size4In, Size3HalfIn, Size3In, Size2In, Size1In,
        Rpm8Third, Rpm16TwoThirds, Rpm33Third, Rpm45, Rpm78, Rpm120,
        Size21Cm, Size25Cm, Size27Cm, Size29Cm, Size35Cm, Size40Cm,
        Size50Cm, Rpm80, Rpm90, IpsFifteenSixteenths, Ips1SevenEighths, Ips15,
        Ips3ThreeQuarters, Ips30, Ips7Half, HalfInchTape, QuarterInchTape, EighthInchTape,
        T2TrackMono, T2TrackStereo, T4TrackMono, T4TrackStereo, NabReel10Point5In, CineReel3In,
        CineReel5In, CineReel6In, CineReel7In, Minute2, Minute3, Minute4,
        Concert, Salon, Mini, BusinessCard, Shape, Minimax,
        CdRom, CdI, CdPlusG, Hdcd, Vcd, Avcd,
        Svcd, Xrcd, Uhd4k, Uhd8k, BluRayAudio, Multichannel,
        DvdAudio, DvdData, DvdVideo, Hybrid, Aac, Aifc,
        Aiff, Alac, Amr, Ape, Avi, Dff,
        DiscImage, Dsf, Flac, Flv, Mov, Mp1,
        Mp2, Mp3, MpegVideo, Mpeg4Video, OggVorbis, Opus,
        Ra, Rm, Shn, Spx, Swf, Tta,
        Wav, WavPack, Wma, Wmv, Mp3Surround, Floppy3Point5In,
        Floppy5Point25In, DoubleSided, SingleSided, Advance, RecordStoreDay, Stereo,
        Mono, Quadraphonic, Ambisonic, Bioplastic, CardBacked, ClubEdition,
        CopyProtected, DeluxeEdition, Enhanced, Etched, Jukebox, LimitedEdition,
        Mispress, Misprint, Numbered, PartiallyMixed, PartiallyUnofficial, PictureDisc,
        Reissue, Remastered, Repress, SpecialCut, SpecialEdition, Styrene,
        TestPressing, TourRecording, Transcription, WhiteLabel, Film16mm, Film35mm,
        Ntsc, Pal, Secam,
    },
}

/// How a surface words one vocabulary value: through its string catalog, or
/// as the term is printed — a proper name, a size, a file type — which reads
/// the same in every language.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTermLabel {
    Localized { key: String },
    Verbatim { text: String },
}

fn localized(key: &str) -> BridgeTermLabel {
    BridgeTermLabel::Localized {
        key: key.to_string(),
    }
}

fn verbatim(text: &str) -> BridgeTermLabel {
    BridgeTermLabel::Verbatim {
        text: text.to_string(),
    }
}

/// One part of a pressing's line, in the order a surface shows them joined
/// by its list separator.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFactTerm {
    /// A country, which the platform names in the reader's language from its
    /// ISO 3166-1 code.
    Country { code: String },
    /// A value worded as its label says.
    ///
    /// Not `Label`: C# nests each variant as a record in the enum's scope, and
    /// a variant named `Label` would shadow `Counted`'s `Label` member's type.
    Worded { label: BridgeTermLabel },
    /// More than one of a medium: the surface words it with
    /// `core.pressing.media_count`, its `count` and the medium's label.
    Counted { count: u32, label: BridgeTermLabel },
}

/// The catalog key naming a region.
#[uniffi::export]
pub fn bridge_region_key(region: BridgeRegion) -> String {
    format!("core.pressing.region.{}", region.into_core().key())
}

/// The catalog key naming a release status.
#[uniffi::export]
pub fn bridge_release_status_key(status: BridgeReleaseStatus) -> String {
    format!("core.pressing.status.{}", status.into_core().key())
}

/// The catalog key naming a packaging.
#[uniffi::export]
pub fn bridge_packaging_key(packaging: BridgePackaging) -> String {
    format!("core.pressing.packaging.{}", packaging.into_core().key())
}

/// How a medium is worded: a family a person has a word for ("vinyl",
/// "cassette") through the catalog, a format's own name ("CD", "Blu-ray") as
/// printed.
#[uniffi::export]
pub fn bridge_medium_label(medium: BridgeMedium) -> BridgeTermLabel {
    match medium {
        BridgeMedium::Cd => verbatim("CD"),
        BridgeMedium::Sacd => verbatim("SACD"),
        BridgeMedium::Dvd => verbatim("DVD"),
        BridgeMedium::HdDvd => verbatim("HD DVD"),
        BridgeMedium::BluRay => verbatim("Blu-ray"),
        BridgeMedium::Vinyl => localized("core.pressing.medium.vinyl"),
        BridgeMedium::Shellac => localized("core.pressing.medium.shellac"),
        BridgeMedium::Acetate => localized("core.pressing.medium.acetate"),
        BridgeMedium::Cassette => localized("core.pressing.medium.cassette"),
        BridgeMedium::Dcc => verbatim("DCC"),
        BridgeMedium::Dat => verbatim("DAT"),
        BridgeMedium::Cartridge => localized("core.pressing.medium.cartridge"),
        BridgeMedium::ReelToReel => localized("core.pressing.medium.reel_to_reel"),
        BridgeMedium::Digital => localized("core.pressing.medium.digital"),
        BridgeMedium::MiniDisc => verbatim("MiniDisc"),
        BridgeMedium::LaserDisc => verbatim("LaserDisc"),
        BridgeMedium::Cdv => verbatim("CDV"),
        BridgeMedium::VideoCd => verbatim("Video CD"),
        BridgeMedium::VideoTape => localized("core.pressing.medium.video_tape"),
        BridgeMedium::FlashMemory => localized("core.pressing.medium.flash_memory"),
        BridgeMedium::Floppy => localized("core.pressing.medium.floppy"),
        BridgeMedium::Cylinder => localized("core.pressing.medium.cylinder"),
        BridgeMedium::DualDisc => verbatim("DualDisc"),
        BridgeMedium::VinylDisc => verbatim("VinylDisc"),
        BridgeMedium::DvdPlus => verbatim("DVDplus"),
        BridgeMedium::CdRecord => verbatim("CD-Record"),
        BridgeMedium::PianoRoll => localized("core.pressing.medium.piano_roll"),
        BridgeMedium::EdisonDisc => verbatim("Edison Disc"),
        BridgeMedium::PatheDisc => verbatim("Pathé Disc"),
        BridgeMedium::Tefifon => verbatim("Tefifon"),
        BridgeMedium::Umd => verbatim("UMD"),
        BridgeMedium::Vhd => verbatim("VHD"),
        BridgeMedium::SelectaVision => verbatim("SelectaVision"),
        BridgeMedium::Ted => verbatim("TeD"),
        BridgeMedium::Mvd => verbatim("MVD"),
        BridgeMedium::WireRecording => localized("core.pressing.medium.wire_recording"),
        BridgeMedium::FilmReel => localized("core.pressing.medium.film_reel"),
        BridgeMedium::MightyTiny => verbatim("Mighty Tiny"),
        BridgeMedium::Sopic => verbatim("Sopic"),
        BridgeMedium::KitAlbum => verbatim("KiT Album"),
        BridgeMedium::RomCartridge => localized("core.pressing.medium.rom_cartridge"),
        BridgeMedium::DataPlay => verbatim("DataPlay"),
    }
}

/// How a Discogs detail is worded: a word ("Reissue") through the catalog, a
/// term ("12\"", "FLAC") as Discogs prints it.
#[uniffi::export]
pub fn bridge_discogs_detail_label(detail: BridgeDiscogsDetail) -> BridgeTermLabel {
    let detail = detail.into_core();
    match detail.wording() {
        bae_core::pressing::discogs_detail::Wording::Word => BridgeTermLabel::Localized {
            key: format!("core.pressing.discogs.{}", detail.key()),
        },
        bae_core::pressing::discogs_detail::Wording::Term => verbatim(detail.term()),
    }
}

/// A pressing's first line: where it was released, then what it is made of —
/// "Japan · 2×CD".
#[uniffi::export]
pub fn bridge_pressing_summary(facts: BridgePressingFacts) -> Vec<BridgeFactTerm> {
    let mut terms: Vec<BridgeFactTerm> = facts
        .area
        .map(|area| match area {
            BridgeReleaseArea::Country { code } => BridgeFactTerm::Country { code },
            BridgeReleaseArea::Region { region } => BridgeFactTerm::Worded {
                label: localized(&bridge_region_key(region)),
            },
        })
        .into_iter()
        .collect();
    terms.extend(bridge_media_terms(facts.media));
    terms
}

/// A pressing's second line: how official it is where that is anything but
/// the ordinary official release, what it is sold in, and the details
/// Discogs states — "Promotion · Reissue".
#[uniffi::export]
pub fn bridge_pressing_details(facts: BridgePressingFacts) -> Vec<BridgeFactTerm> {
    let status = facts
        .status
        .filter(|status| *status != BridgeReleaseStatus::Official)
        .map(|status| localized(&bridge_release_status_key(status)));
    let packaging = facts
        .packaging
        .map(|packaging| localized(&bridge_packaging_key(packaging)));
    status
        .into_iter()
        .chain(packaging)
        .chain(
            facts
                .discogs_details
                .into_iter()
                .map(bridge_discogs_detail_label),
        )
        .map(|label| BridgeFactTerm::Worded { label })
        .collect()
}

/// Media as a line's parts, each carrier once with its count — "2×CD",
/// "DVD".
#[uniffi::export]
pub fn bridge_media_terms(media: Vec<BridgeMediaCount>) -> Vec<BridgeFactTerm> {
    media
        .into_iter()
        .map(|counted| {
            let label = bridge_medium_label(counted.medium);
            match counted.count {
                1 => BridgeFactTerm::Worded { label },
                count => BridgeFactTerm::Counted { count, label },
            }
        })
        .collect()
}

/// Every ISO 3166-1 country code, for a picker to name and sort.
#[uniffi::export]
pub fn bridge_country_codes() -> Vec<String> {
    bae_core::pressing::Country::all()
        .map(|country| country.code().to_string())
        .collect()
}

/// Every region, for a picker.
#[uniffi::export]
pub fn bridge_regions() -> Vec<BridgeRegion> {
    bae_core::pressing::Region::ALL
        .iter()
        .map(|region| BridgeRegion::from_core(*region))
        .collect()
}

/// Every medium, for a picker.
#[uniffi::export]
pub fn bridge_media() -> Vec<BridgeMedium> {
    bae_core::pressing::Medium::ALL
        .iter()
        .map(|medium| BridgeMedium::from_core(*medium))
        .collect()
}

/// Every release status, for a picker.
#[uniffi::export]
pub fn bridge_release_statuses() -> Vec<BridgeReleaseStatus> {
    bae_core::pressing::ReleaseStatus::ALL
        .iter()
        .map(|status| BridgeReleaseStatus::from_core(*status))
        .collect()
}

/// Every packaging, for a picker.
#[uniffi::export]
pub fn bridge_packagings() -> Vec<BridgePackaging> {
    bae_core::pressing::Packaging::ALL
        .iter()
        .map(|packaging| BridgePackaging::from_core(*packaging))
        .collect()
}

/// Every Discogs detail, for a picker.
#[uniffi::export]
pub fn bridge_discogs_details() -> Vec<BridgeDiscogsDetail> {
    bae_core::pressing::DiscogsDetail::ALL
        .iter()
        .map(|detail| BridgeDiscogsDetail::from_core(*detail))
        .collect()
}

/// What a list of an album's releases calls one of them. Mirrors
/// `bae_core::album_detail::ReleaseName`; each surface words it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseName {
    /// The name the person gave it.
    Named { name: String },
    /// No name: its year and media, whichever it states. The surface joins
    /// the year and `bridge_media_terms(media)`.
    Described {
        year: Option<i32>,
        media: Vec<BridgeMediaCount>,
    },
    /// No name and nothing to describe it by: its place among the album's
    /// releases, counted from one, worded with `core.release.numbered`.
    Numbered { number: i64 },
}

impl BridgeReleaseName {
    pub(crate) fn from_core(name: bae_core::album_detail::ReleaseName) -> Self {
        match name {
            bae_core::album_detail::ReleaseName::Named(name) => Self::Named { name },
            bae_core::album_detail::ReleaseName::Described { year, media } => Self::Described {
                year,
                media: media.into_iter().map(BridgeMediaCount::from_core).collect(),
            },
            bae_core::album_detail::ReleaseName::Numbered(number) => Self::Numbered { number },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn japan_cd() -> BridgePressingFacts {
        BridgePressingFacts {
            area: Some(BridgeReleaseArea::Country {
                code: "JP".to_string(),
            }),
            media: vec![BridgeMediaCount {
                medium: BridgeMedium::Cd,
                count: 1,
            }],
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
        }
    }

    /// Both catalogs' rows read the same shape: the place, then the media.
    #[test]
    fn a_summary_is_the_place_then_the_media() {
        assert_eq!(
            bridge_pressing_summary(japan_cd()),
            vec![
                BridgeFactTerm::Country {
                    code: "JP".to_string()
                },
                BridgeFactTerm::Worded {
                    label: verbatim("CD")
                },
            ]
        );
        let two_vinyl_in_europe = BridgePressingFacts {
            area: Some(BridgeReleaseArea::Region {
                region: BridgeRegion::UkAndEurope,
            }),
            media: vec![BridgeMediaCount {
                medium: BridgeMedium::Vinyl,
                count: 2,
            }],
            ..japan_cd()
        };
        assert_eq!(
            bridge_pressing_summary(two_vinyl_in_europe),
            vec![
                BridgeFactTerm::Worded {
                    label: localized("core.pressing.region.uk_and_europe")
                },
                BridgeFactTerm::Counted {
                    count: 2,
                    label: localized("core.pressing.medium.vinyl")
                },
            ]
        );
    }

    /// The detail line names what sets a pressing apart; an ordinary
    /// official release is not set apart by being official.
    #[test]
    fn details_leave_out_the_ordinary_official_status() {
        let promo = BridgePressingFacts {
            status: Some(BridgeReleaseStatus::Promotion),
            packaging: Some(BridgePackaging::Digipak),
            discogs_details: vec![BridgeDiscogsDetail::Reissue, BridgeDiscogsDetail::Flac],
            ..japan_cd()
        };
        assert_eq!(
            bridge_pressing_details(promo),
            vec![
                BridgeFactTerm::Worded {
                    label: localized("core.pressing.status.promotion")
                },
                BridgeFactTerm::Worded {
                    label: localized("core.pressing.packaging.digipak")
                },
                BridgeFactTerm::Worded {
                    label: localized("core.pressing.discogs.reissue")
                },
                BridgeFactTerm::Worded {
                    label: verbatim("FLAC")
                },
            ]
        );
        let official = BridgePressingFacts {
            status: Some(BridgeReleaseStatus::Official),
            ..japan_cd()
        };
        assert!(bridge_pressing_details(official).is_empty());
    }
}
