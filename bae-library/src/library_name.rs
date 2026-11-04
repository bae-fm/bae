use rand::prelude::IteratorRandom;

/// One word per beat, whitespace-separated: the list is read by splitting it.
const VERBS: &str = concat!(
    "boppin groovin swingin rockin jigging vibin jammin funkin chillin ",
    "cruisin bumpin rollin flowin blazin rippin shreddin stompin thumpin ",
    "bouncin struttin slidin tappin hummin wailin mixin looping droppin ",
    "spinnin scratchin ticklin strummin pluckin beltin snappin poppin ",
    "buskin noodlin howlin swooning crooning twangin riffin sampling ",
    "beatboxin freestylin headbangin",
);

/// Whitespace-separated like [`VERBS`], grouped by the music they made.
const MUSICIANS: &str = concat!(
    // classical
    "bach beethoven brahms chopin debussy gershwin grieg holst liszt ",
    "mahler mozart paganini ravel satie schubert stravinsky tchaikovsky ",
    "vivaldi ",
    // jazz
    "coltrane davis dizzy ella ellington mingus monk ",
    // rock / pop / funk / electronic
    "aretha billie bjork bowie dolly elvis etta hendrix marley nina otis ",
    "prince sinatra stevie sting waits zappa ",
    // hip-hop / rap
    "dilla kendrick lauryn missy nas outkast questlove tupac ",
    // r&b / soul / gospel
    "erykah luther sade sam-cooke whitney ",
    // country / folk
    "cash joni woody ",
    // latin / brazilian
    "celia gilberto jobim piazzolla santana selena shakira tito ",
    // south asian
    "bismillah lata nusrat shankar zakir ",
    // east asian
    "kitaro ryuichi yo-yo ",
    // african
    "fela miriam youssou",
);

/// Generate a fun default library name like "groovin-coltrane" or "boppin-beethoven".
pub fn generate_library_name() -> bae_db::EnteredName {
    let mut rng = rand::rng();
    let mut word = |list: &str| {
        list.split_whitespace().choose(&mut rng).expect("the word lists are not empty").to_string()
    };
    let verb = word(VERBS);
    let musician = word(MUSICIANS);
    bae_db::EnteredName::parse(&format!("{verb}-{musician}"))
        .expect("generated library names are always non-blank")
}
