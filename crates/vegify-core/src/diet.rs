//! WORD-AWARE diet matching over branded-food ingredient statements.
//!
//! Why this module exists as its own thing, and why it is word-aware rather than a substring scan:
//! the moment we start carrying somebody else's branded text (product names, ingredient statements,
//! allergen strings), a naive `text.contains("milk")` flags "Coconut milk" as dairy. That is not a
//! hypothetical — a competitor shipped exactly that bug, and it is the kind of miss that destroys
//! trust in a vegan flag permanently: one false dairy hit on an obviously-vegan product and the user
//! stops believing every flag after it. So the matcher here operates on WORD SPANS, never on raw
//! substrings, and then applies two suppression passes on top:
//!
//! 1. **Preceding plant qualifier** — "coconut"/"oat"/"almond"/"soy"… before a dairy word means the
//!    word names a plant product, not an animal one ("coconut milk", "cashew cheese", "oat creamer").
//!    Word boundaries alone do NOT fix this: "milk" genuinely IS a whole word inside "Coconut milk".
//! 2. **Following vegan collocation** — some animal words are innocent because of what comes AFTER
//!    them ("cream of tartar", "milk thistle", "butter lettuce", "butter beans").
//!
//! Tokenizing also fixes the pure-substring class for free: "butternut" is one token and never
//! matches "butter"; "eggplant" never matches "egg".
//!
//! **These flags are ADVISORY, and deliberately one-directional.** [`animal_derived_flags`] reports
//! what it FOUND; it never returns "this is vegan". Absence of a flag is absence of evidence, not a
//! certification — third-party ingredient statements are incomplete, inconsistently worded, and
//! frequently stale. The UI must present a flag as "contains, per the label" and must never present
//! an empty result as a vegan guarantee. That honesty is the product position, not a hedge.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What kind of animal product a flagged term names — the grouping the UI renders and the reason a
/// flag is worth surfacing separately (someone avoiding dairy is not necessarily avoiding honey).
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DietCategory {
    /// Milk-derived (milk, butter, cheese, cream, whey, casein, lactose, ghee).
    Dairy,
    /// Hen/other egg components (egg, albumen, ovalbumin).
    Egg,
    /// Land-animal flesh or its rendered/extracted products (gelatin, lard, tallow, collagen, rennet).
    Meat,
    /// Aquatic animals (fish, anchovy, shellfish).
    Fish,
    /// Insect-derived (honey, beeswax, shellac, carmine/cochineal).
    Insect,
}

/// One animal-derived term found in an ingredient statement, with the exact text that matched so the
/// UI can show the user WHERE the flag came from rather than asserting a verdict at them.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DietFlag {
    /// The canonical term that matched (lowercase, e.g. "milk").
    pub term: String,
    /// The matched span exactly as it appears in the source statement (e.g. "Milk"), so the client can
    /// quote the label instead of paraphrasing it.
    pub matched: String,
    /// Which animal-product family the term belongs to.
    pub category: DietCategory,
}

/// Animal-derived terms, as WORD SEQUENCES (a term may be several words — matched as a contiguous
/// token run, so "sodium caseinate" needs both words adjacent). Kept deliberately to the unambiguous
/// cases: terms with a common vegan reading that qualifiers can't disambiguate (mono- and
/// diglycerides, "natural flavors", vitamin D3, L-cysteine) are NOT listed, because a maybe-flag is
/// worse than no flag — it trains users to ignore the panel.
const ANIMAL_TERMS: &[(&[&str], DietCategory)] = &[
    // dairy
    (&["milk"], DietCategory::Dairy),
    (&["butter"], DietCategory::Dairy),
    (&["buttermilk"], DietCategory::Dairy),
    (&["cheese"], DietCategory::Dairy),
    (&["cream"], DietCategory::Dairy),
    (&["yogurt"], DietCategory::Dairy),
    (&["yoghurt"], DietCategory::Dairy),
    (&["whey"], DietCategory::Dairy),
    (&["casein"], DietCategory::Dairy),
    (&["caseinate"], DietCategory::Dairy),
    (&["lactose"], DietCategory::Dairy),
    (&["ghee"], DietCategory::Dairy),
    (&["skyr"], DietCategory::Dairy),
    (&["kefir"], DietCategory::Dairy),
    (&["quark"], DietCategory::Dairy),
    (&["mascarpone"], DietCategory::Dairy),
    (&["ricotta"], DietCategory::Dairy),
    (&["paneer"], DietCategory::Dairy),
    // egg
    (&["egg"], DietCategory::Egg),
    (&["eggs"], DietCategory::Egg),
    (&["albumen"], DietCategory::Egg),
    (&["ovalbumin"], DietCategory::Egg),
    // meat / slaughter by-products
    (&["gelatin"], DietCategory::Meat),
    (&["gelatine"], DietCategory::Meat),
    (&["lard"], DietCategory::Meat),
    (&["tallow"], DietCategory::Meat),
    (&["collagen"], DietCategory::Meat),
    (&["rennet"], DietCategory::Meat),
    (&["beef"], DietCategory::Meat),
    (&["pork"], DietCategory::Meat),
    (&["chicken"], DietCategory::Meat),
    // fish / shellfish
    (&["fish"], DietCategory::Fish),
    (&["anchovy"], DietCategory::Fish),
    (&["anchovies"], DietCategory::Fish),
    (&["shellfish"], DietCategory::Fish),
    (&["shrimp"], DietCategory::Fish),
    // insect
    (&["honey"], DietCategory::Insect),
    (&["beeswax"], DietCategory::Insect),
    (&["shellac"], DietCategory::Insect),
    (&["carmine"], DietCategory::Insect),
    (&["cochineal"], DietCategory::Insect),
];

/// Words that, immediately BEFORE an animal term, name the plant the product is made from — so the
/// term describes a plant analogue, not an animal ingredient. THIS is the "Coconut milk" fix: word
/// boundaries are necessary but not sufficient, because "milk" really is a whole word there.
const PLANT_QUALIFIERS: &[&str] = &[
    "coconut",
    "almond",
    "soy",
    "soya",
    "oat",
    "rice",
    "cashew",
    "hemp",
    "flax",
    "flaxseed",
    "pea",
    "hazelnut",
    "macadamia",
    "walnut",
    "pecan",
    "pistachio",
    "peanut",
    "sesame",
    "sunflower",
    "cocoa",
    "cacao",
    "shea",
    "coco",
    "nut",
    "seed",
    "quinoa",
    "chickpea",
    "potato",
    "apple",
    "banana",
    "mango",
    "avocado",
    "olive",
    "palm",
    "vegan",
    "vegetable",
    "plant",
    "veggie",
    "faux",
    "imitation",
    "alternative",
    "substitute",
    "water",
];

/// Two-word qualifier phrases (the preceding PAIR of tokens), for the plant markers that are written
/// as two words on a label: "non dairy creamer", "plant based cheese", "dairy free butter".
const PLANT_QUALIFIER_PHRASES: &[[&str; 2]] = &[
    ["non", "dairy"],
    ["dairy", "free"],
    ["plant", "based"],
    ["nut", "based"],
    ["oat", "based"],
];

/// Words that, immediately AFTER an animal term, make the phrase a plant one — the collocations where
/// the animal word is borrowed as a name ("cream of tartar", "milk thistle", "butter lettuce",
/// "butter beans", "egg plant" when written as two words).
const VEGAN_FOLLOWERS: &[(&str, &str)] = &[
    ("cream", "of"),
    ("milk", "thistle"),
    ("butter", "lettuce"),
    ("butter", "bean"),
    ("butter", "beans"),
    ("butter", "nut"),
    ("egg", "plant"),
    ("fish", "free"),
    ("milk", "free"),
];

/// One token of the statement: the lowercased word plus its byte span in the ORIGINAL text (so a flag
/// can quote the label's own casing back).
struct Token {
    lower: String,
    start: usize,
    end: usize,
}

/// Split `text` into alphanumeric word tokens, dropping punctuation and whitespace. This is the whole
/// difference between "word-aware" and "substring": every match below is a match on a token sequence,
/// so "butternut" can never satisfy "butter" and "eggplant" can never satisfy "egg".
fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push(Token {
                lower: text[s..i].to_lowercase(),
                start: s,
                end: i,
            });
        }
    }
    if let Some(s) = start {
        out.push(Token {
            lower: text[s..].to_lowercase(),
            start: s,
            end: text.len(),
        });
    }
    out
}

/// Is the animal term occupying tokens `[at, at + len)` neutralized by its neighbours?
fn suppressed(tokens: &[Token], at: usize, len: usize, term: &str) -> bool {
    // 1. a plant qualifier immediately before ("coconut milk").
    if at > 0 && PLANT_QUALIFIERS.contains(&tokens[at - 1].lower.as_str()) {
        return true;
    }
    // 2. a two-word plant qualifier phrase immediately before ("non dairy creamer").
    if at > 1 {
        let pair = [tokens[at - 2].lower.as_str(), tokens[at - 1].lower.as_str()];
        if PLANT_QUALIFIER_PHRASES.contains(&pair) {
            return true;
        }
    }
    // 3. a vegan collocation immediately after ("cream of tartar").
    let after = at + len;
    if after < tokens.len() {
        let next = tokens[after].lower.as_str();
        if VEGAN_FOLLOWERS
            .iter()
            .any(|(t, f)| *t == term && *f == next)
        {
            return true;
        }
    }
    false
}

/// Scan a branded food's ingredient statement (or product name) for animal-derived terms, matched as
/// WORDS and suppressed by plant qualifiers/collocations — see the module docs for why every part of
/// that sentence is load-bearing.
///
/// Returns one flag per DISTINCT term found, in first-appearance order (a statement that says "milk"
/// four times yields one dairy flag, not four). An empty result means "nothing recognized", NEVER
/// "certified vegan" — callers must not invert it into a positive claim.
pub fn animal_derived_flags(text: &str) -> Vec<DietFlag> {
    let tokens = tokenize(text);
    let mut out: Vec<DietFlag> = Vec::new();
    for i in 0..tokens.len() {
        for (words, category) in ANIMAL_TERMS {
            let len = words.len();
            if i + len > tokens.len() {
                continue;
            }
            if !(0..len).all(|k| tokens[i + k].lower == words[k]) {
                continue;
            }
            let term = words.join(" ");
            if suppressed(&tokens, i, len, &term) {
                continue;
            }
            if out.iter().any(|f| f.term == term) {
                continue;
            }
            out.push(DietFlag {
                matched: text[tokens[i].start..tokens[i + len - 1].end].to_string(),
                term,
                category: *category,
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    fn terms(text: &str) -> Vec<String> {
        animal_derived_flags(text)
            .into_iter()
            .map(|f| f.term)
            .collect()
    }

    /// THE regression this module exists for: the competitor bug that flagged dairy on "Coconut milk"
    /// because the filter substring-matched the word. Word boundaries alone don't fix it (milk IS a
    /// word here) — the preceding plant qualifier is what does.
    #[test]
    fn plant_milks_are_not_dairy() {
        for statement in [
            "Coconut milk",
            "Organic almond milk, sea salt",
            "OAT MILK (WATER, OATS)",
            "Soy milk",
            "Cashew milk beverage",
            "Non dairy creamer",
            "Plant based cheese shreds",
        ] {
            assert!(
                terms(statement).is_empty(),
                "{statement} must not flag: {:?}",
                terms(statement)
            );
        }
    }

    /// The other half of the contract: real dairy still flags, including when a modifier precedes it
    /// that ISN'T a plant ("nonfat milk", "milk chocolate", "whole milk powder").
    #[test]
    fn real_dairy_still_flags() {
        for statement in [
            "Milk chocolate",
            "Nonfat milk, sugar",
            "Whole milk powder",
            "Cultured cream, salt",
            "Whey protein concentrate",
        ] {
            assert!(!terms(statement).is_empty(), "{statement} must flag dairy");
        }
        assert_eq!(
            animal_derived_flags("Milk chocolate")[0].category,
            DietCategory::Dairy
        );
    }

    /// Tokenization alone kills the classic substring false positives — no qualifier table needed.
    #[test]
    fn substring_lookalikes_never_match() {
        for statement in [
            "Butternut squash, water",
            "Eggplant, olive oil",
            "Buttercup squash",
            "Cocoa butter",    // qualifier suppression
            "Peanut butter",   // qualifier suppression
            "Shea butter",     // qualifier suppression
            "Cream of tartar", // follower suppression
            "Milk thistle extract",
            "Butter lettuce",
        ] {
            assert!(
                terms(statement).is_empty(),
                "{statement} must not flag: {:?}",
                terms(statement)
            );
        }
        // …but "buttermilk" IS dairy, and it's its own token, so it must still flag.
        assert_eq!(terms("Buttermilk powder"), vec!["buttermilk".to_string()]);
    }

    #[test]
    fn categories_and_dedupe() {
        let flags = animal_derived_flags(
            "Sugar, honey, gelatin, egg white, milk, milk solids, anchovy paste",
        );
        let found: Vec<_> = flags.iter().map(|f| f.term.as_str()).collect();
        assert_eq!(found, ["honey", "gelatin", "egg", "milk", "anchovy"]);
        assert_eq!(flags[0].category, DietCategory::Insect);
        assert_eq!(flags[1].category, DietCategory::Meat);
        assert_eq!(flags[2].category, DietCategory::Egg);
        assert_eq!(flags[3].category, DietCategory::Dairy);
        assert_eq!(flags[4].category, DietCategory::Fish);
    }

    /// The flag quotes the LABEL's own casing back, so the UI can show the source text verbatim.
    #[test]
    fn matched_span_preserves_source_casing() {
        let flags = animal_derived_flags("SUGAR, WHEY, SALT");
        assert_eq!(flags[0].matched, "WHEY");
        assert_eq!(flags[0].term, "whey");
    }

    #[test]
    fn fermented_dairy_products_flag() {
        for (statement, expected) in [
            ("SKYR, SUGAR, STRAWBERRIES", "skyr"),
            ("Whole milk kefir, sugar", "kefir"),
            ("Mascarpone cheese, sugar, eggs", "mascarpone"),
            ("Ricotta cheese, salt", "ricotta"),
            ("Quark, honey, oats", "quark"),
            ("Paneer, spices, oil", "paneer"),
        ] {
            let found = terms(statement);
            assert!(
                found.contains(&expected.to_string()),
                "{statement} must flag {expected}, got: {found:?}"
            );
        }
        for statement in [
            "SKYR, SUGAR, STRAWBERRIES",
            "Kefir, fruit",
            "Mascarpone cheese",
            "Ricotta, salt",
            "Quark, honey",
            "Paneer, oil",
        ] {
            assert_eq!(
                animal_derived_flags(statement)[0].category,
                DietCategory::Dairy,
                "{statement} must be Dairy"
            );
        }
    }

    #[test]
    fn water_kefir_is_not_dairy() {
        assert!(
            terms("Water kefir, organic sugar, ginger").is_empty(),
            "water kefir must not flag"
        );
        assert!(
            !terms("Whole milk kefir").is_empty(),
            "milk kefir must flag"
        );
    }

    #[test]
    fn empty_and_punctuation_only_are_clean() {
        assert!(animal_derived_flags("").is_empty());
        assert!(animal_derived_flags("—, . () ").is_empty());
    }
}
