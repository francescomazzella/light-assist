//! The random legible part of a help address. Easy to say and spell; it is not a secret.
//!
//! There are enough words that two computers on one network are unlikely to pick the same address,
//! which matters most for an address that is kept: a computer with unattended access uses three words.

const FIRST: &[&str] = &[
    "amber", "ample", "apple", "april", "arctic", "autumn", "azure", "balmy", "berry", "bold", "brave", "breezy",
    "bright", "brisk", "bronze", "calm", "candid", "cedar", "cheery", "chief", "civil", "clean", "clear", "clever",
    "cloudy", "cobalt", "cool", "copper", "coral", "cosy", "crisp", "curly", "dainty", "dapper", "dawn", "deep",
    "dusky", "eager", "early", "earthy", "easy", "elder", "even", "fair", "fancy", "fine", "fleet", "fond", "frank",
    "fresh", "frosty", "gentle", "giant", "glad", "golden", "grand", "green", "handy", "happy", "hardy", "hazel",
    "honest", "humble", "ivory", "jade", "jolly", "keen", "kind", "lemon", "light", "lilac", "lively", "loyal",
    "lucky", "lunar", "maple", "mellow", "merry", "mighty", "mild", "minty", "misty", "modest", "neat", "noble",
    "north", "olive", "opal", "orange", "pastel", "patient", "peach", "pearl", "plain", "plucky", "polar", "polite",
    "proud", "purple", "quick", "quiet", "rapid", "ready", "regal", "robust", "rosy", "royal", "ruby", "rustic",
    "sandy", "scarlet", "shiny", "silent", "silver", "simple", "smooth", "snowy", "soft", "solar", "solid", "sound",
    "south", "spring", "stable", "steady", "still", "stout", "sturdy", "sunny", "super", "swift", "tender", "tidy",
    "timely", "trusty", "upper", "velvet", "vivid", "warm", "west", "wide", "windy", "winter", "wise", "witty",
    "young", "zesty",
];

const SECOND: &[&str] = &[
    "acorn", "anchor", "arbor", "arch", "aspen", "atlas", "badger", "bank", "barn", "basin", "bay", "beach", "beacon",
    "bear", "beaver", "birch", "bird", "bluff", "boat", "bridge", "brook", "cabin", "canal", "canyon", "cape",
    "castle", "cedar", "chapel", "cliff", "cloud", "clover", "coast", "comet", "coral", "cove", "crane", "creek",
    "crest", "dale", "delta", "desert", "dome", "dove", "dune", "eagle", "elm", "ember", "falcon", "farm", "fern",
    "ferry", "field", "finch", "fjord", "forest", "fountain", "fox", "garden", "gate", "glade", "glen", "grove",
    "gull", "harbor", "haven", "hawk", "heron", "hill", "hollow", "island", "ivy", "jetty", "lagoon", "lake",
    "lantern", "lark", "leaf", "ledge", "lily", "lodge", "maple", "marsh", "meadow", "mesa", "mill", "moon",
    "morning", "moss", "mountain", "oak", "oasis", "ocean", "orchard", "otter", "owl", "palm", "park", "pass", "path",
    "peak", "pebble", "pier", "pine", "plain", "planet", "plaza", "pond", "port", "prairie", "quarry", "rain",
    "rapids", "reef", "ridge", "river", "road", "robin", "rock", "sail", "sand", "shore", "sky", "slope", "spring",
    "spruce", "star", "stone", "stream", "summit", "swan", "thicket", "tide", "tower", "trail", "tree", "tulip",
    "valley", "village", "vista", "water", "wave", "willow", "wind", "wood", "wren", "yard",
];

fn pick(words: &[&'static str]) -> &'static str {
    let n = getrandom::u32().expect("OS randomness is unavailable");
    words[n as usize % words.len()]
}

/// The part of an address that lasts for one help request, such as `maple-river`.
pub fn pair() -> String {
    format!("{}-{}", pick(FIRST), pick(SECOND))
}

/// The part of an address a computer keeps, such as `calm-maple-river`: one of a couple of million.
pub fn lasting() -> String {
    loop {
        let (one, two) = (pick(FIRST), pick(FIRST));
        if one != two {
            return format!("{one}-{two}-{}", pick(SECOND));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_is_fit_for_an_address_and_listed_once() {
        for words in [FIRST, SECOND] {
            assert!(words.len() >= 128);
            for (index, word) in words.iter().enumerate() {
                assert!(word.len() <= 8 && word.bytes().all(|byte| byte.is_ascii_lowercase()), "{word}");
                assert!(!words[..index].contains(word), "{word} is listed twice");
            }
        }
        assert_eq!(lasting().split('-').count(), 3);
    }
}
