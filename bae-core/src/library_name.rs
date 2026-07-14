use rand::prelude::IndexedRandom;

/// A validated, non-blank library name. The one definition of "a library name is
/// non-empty": [`parse`](Self::parse) trims and rejects a blank string, and every
/// path that creates or renames a library takes this type rather than a bare
/// `String`, so the rule cannot be stated three different ways (or skipped) at
/// three different call sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryName(String);

/// A library name was empty (or all whitespace) after trimming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Library name cannot be empty")]
pub struct EmptyLibraryName;

impl LibraryName {
    /// Trim `raw` and return the name, or [`EmptyLibraryName`] when nothing is
    /// left. The single gate every surface routes a user-entered name through.
    pub fn parse(raw: &str) -> Result<Self, EmptyLibraryName> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Err(EmptyLibraryName)
        } else {
            Ok(Self(trimmed.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// Generate a fun default library name like "groovin-coltrane" or "boppin-beethoven".
pub fn generate_library_name() -> LibraryName {
    const VERBS: &[&str] = &[
        "boppin",
        "groovin",
        "swingin",
        "rockin",
        "jigging",
        "vibin",
        "jammin",
        "funkin",
        "chillin",
        "cruisin",
        "bumpin",
        "rollin",
        "flowin",
        "blazin",
        "rippin",
        "shreddin",
        "stompin",
        "thumpin",
        "bouncin",
        "struttin",
        "slidin",
        "tappin",
        "hummin",
        "wailin",
        "mixin",
        "looping",
        "droppin",
        "spinnin",
        "scratchin",
        "ticklin",
        "strummin",
        "pluckin",
        "beltin",
        "snappin",
        "poppin",
        "buskin",
        "noodlin",
        "howlin",
        "swooning",
        "crooning",
        "twangin",
        "riffin",
        "sampling",
        "beatboxin",
        "freestylin",
        "headbangin",
    ];
    const MUSICIANS: &[&str] = &[
        // classical
        "bach",
        "beethoven",
        "brahms",
        "chopin",
        "debussy",
        "gershwin",
        "grieg",
        "holst",
        "liszt",
        "mahler",
        "mozart",
        "paganini",
        "ravel",
        "satie",
        "schubert",
        "stravinsky",
        "tchaikovsky",
        "vivaldi",
        // jazz
        "coltrane",
        "davis",
        "dizzy",
        "ella",
        "ellington",
        "mingus",
        "monk",
        // rock / pop / funk / electronic
        "aretha",
        "billie",
        "bjork",
        "bowie",
        "dolly",
        "elvis",
        "etta",
        "hendrix",
        "marley",
        "nina",
        "otis",
        "prince",
        "sinatra",
        "stevie",
        "sting",
        "waits",
        "zappa",
        // hip-hop / rap
        "dilla",
        "kendrick",
        "lauryn",
        "missy",
        "nas",
        "outkast",
        "questlove",
        "tupac",
        // r&b / soul / gospel
        "erykah",
        "luther",
        "sade",
        "sam-cooke",
        "whitney",
        // country / folk
        "cash",
        "joni",
        "woody",
        // latin / brazilian
        "celia",
        "gilberto",
        "jobim",
        "piazzolla",
        "santana",
        "selena",
        "shakira",
        "tito",
        // south asian
        "bismillah",
        "lata",
        "nusrat",
        "shankar",
        "zakir",
        // east asian
        "kitaro",
        "ryuichi",
        "yo-yo",
        // african
        "fela",
        "miriam",
        "youssou",
    ];
    let mut rng = rand::rng();
    let verb = VERBS.choose(&mut rng).unwrap();
    let musician = MUSICIANS.choose(&mut rng).unwrap();
    LibraryName::parse(&format!("{verb}-{musician}"))
        .expect("generated library names are always non-blank")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_whitespace_are_rejected() {
        assert_eq!(LibraryName::parse(""), Err(EmptyLibraryName));
        assert_eq!(LibraryName::parse("   "), Err(EmptyLibraryName));
        assert_eq!(LibraryName::parse("\t\n"), Err(EmptyLibraryName));
    }

    #[test]
    fn a_name_is_trimmed() {
        let name = LibraryName::parse("  Jazz  ").expect("non-blank");
        assert_eq!(name.as_str(), "Jazz");
    }

    #[test]
    fn generated_names_are_non_blank() {
        assert!(!generate_library_name().as_str().is_empty());
    }
}
