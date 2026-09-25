//! Spanish for the interface text upstream has not localized yet.
//!
//! Upstream translates a string by wrapping it in `crate::i18n::gettext` at
//! the call site, with the locale in hand, and reads the catalogs in
//! `assets/i18n`. Most of its interface is still plain English literals, and
//! `es.po` is upstream's, rewritten by `msgmerge` on every sync. So the fork
//! keeps its translations in its own table, `i18n/es.rs`, and upstream files
//! reach it with one-line hooks:
//!
//! - helpers that draw a caller's text (`theme::text`, `menu_item`,
//!   `setting_row`, `soft_button`, toasts, …) pass it through [`tr`] or
//!   [`tr_string`], which covers every call site at once;
//! - a literal drawn straight through egui is wrapped as
//!   `crate::fork::i18n::tr("…")`;
//! - `crate::i18n::gettext` falls back to the table for strings upstream marked
//!   but left untranslated in `es.po`.
//!
//! A table entry whose English has `{}` is a pattern: [`tr_string`] matches
//! text built with `format!` against it and carries the pieces over, so
//! `"Could not record: {}"` also translates `"Could not record: busy"`. The
//! Spanish takes the pieces in order, or by position as `{1}`, `{2}`, ….
//!
//! [`tr`] needs no locale argument: the account on screen sets it each frame,
//! so hooks never have to thread one through upstream code. Text that is not
//! in the table, including chat names and messages that happen to reach a
//! helper, comes back unchanged.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::i18n::Locale;

mod es;

/// Whether the account on screen shows its interface in Spanish.
static SPANISH: AtomicBool = AtomicBool::new(false);

static TABLE: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| es::TABLE.iter().copied().collect());

/// Entries with `{}`, split at each placeholder.
static PATTERNS: LazyLock<Vec<(Vec<&'static str>, &'static str)>> = LazyLock::new(|| {
    es::TABLE
        .iter()
        .filter(|(source, _)| source.contains("{}") && source.trim() != "{}")
        .map(|&(source, spanish)| (source.split("{}").collect(), spanish))
        .collect()
});

/// Follows the interface language of the account on screen.
pub fn show(locale: Locale) {
    SPANISH.store(locale == Locale::Spanish, Ordering::Relaxed);
}

fn spanish_on_screen() -> bool {
    SPANISH.load(Ordering::Relaxed)
}

/// The Spanish for `source` in `locale`, if the fork table has it.
pub fn spanish(locale: Locale, source: &str) -> Option<&'static str> {
    (locale == Locale::Spanish)
        .then(|| TABLE.get(source).copied())
        .flatten()
}

/// Translates interface text into the language on screen, leaving anything
/// the table does not know as it is.
pub fn tr(source: &str) -> &str {
    if spanish_on_screen() {
        spanish(Locale::Spanish, source).unwrap_or(source)
    } else {
        source
    }
}

/// [`tr`] for owned text, which may also have been built from a pattern.
pub fn tr_string(text: String) -> String {
    if !spanish_on_screen() {
        return text;
    }
    if let Some(spanish) = TABLE.get(text.as_str()) {
        return (*spanish).to_owned();
    }
    PATTERNS
        .iter()
        .find_map(|(pieces, spanish)| fill(spanish, &captures(pieces, &text)?))
        .unwrap_or(text)
}

/// What `text` has in place of each placeholder between `pieces`, if it
/// matches them. Each capture is as short as the next piece allows.
fn captures<'a>(pieces: &[&str], text: &'a str) -> Option<Vec<&'a str>> {
    let (first, rest) = pieces.split_first()?;
    let (last, middle) = rest.split_last()?;
    let mut remaining = text.strip_prefix(first)?;
    let mut found = Vec::with_capacity(rest.len());
    for piece in middle {
        let at = remaining.find(piece).filter(|_| !piece.is_empty())?;
        found.push(&remaining[..at]);
        remaining = &remaining[at + piece.len()..];
    }
    found.push(remaining.strip_suffix(last)?);
    found.iter().all(|capture| !capture.is_empty()).then_some(found)
}

/// The Spanish pattern with the captures in its `{}` or `{N}` slots.
fn fill(spanish: &str, captures: &[&str]) -> Option<String> {
    let mut out = String::with_capacity(spanish.len() + 16);
    let mut next = 0;
    let mut rest = spanish;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = open + rest[open..].find('}')?;
        let slot = &rest[open + 1..close];
        let index = if slot.is_empty() {
            next += 1;
            next - 1
        } else {
            slot.parse::<usize>().ok()?.checked_sub(1)?
        };
        let capture = *captures.get(index)?;
        out.push_str(TABLE.get(capture).copied().unwrap_or(capture));
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(text: &str) -> usize {
        text.matches('{').count()
    }

    #[test]
    fn the_table_has_no_duplicates_or_blanks() {
        let mut seen = std::collections::HashSet::new();
        for (source, spanish) in es::TABLE {
            assert!(seen.insert(source), "duplicate: {source}");
            assert!(!spanish.trim().is_empty(), "blank: {source}");
        }
    }

    #[test]
    fn placeholders_survive_translation() {
        for (source, spanish) in es::TABLE {
            assert_eq!(slots(source), slots(spanish), "{source} → {spanish}");
            if source.contains("{}") {
                let sample: Vec<&str> = (0..slots(source)).map(|_| "x").collect();
                assert!(fill(spanish, &sample).is_some(), "{spanish}");
            }
        }
    }

    #[test]
    fn only_spanish_reads_the_table() {
        assert_eq!(spanish(Locale::Spanish, "Save"), Some("Guardar"));
        assert_eq!(spanish(Locale::English, "Save"), None);
        assert_eq!(spanish(Locale::Spanish, "Somebody's name"), None);
    }

    #[test]
    fn patterns_carry_the_pieces_over() {
        assert_eq!(
            captures(&["Could not record: ", ""], "Could not record: busy"),
            Some(vec!["busy"])
        );
        assert_eq!(captures(&["Could not record: ", ""], "Could not record: "), None);
        assert_eq!(
            captures(&["", " of ", ""], "3 of 10"),
            Some(vec!["3", "10"])
        );
        assert_eq!(fill("{2} de {1}", &["a", "b"]), Some("b de a".to_owned()));
        assert_eq!(fill("{} de {}", &["a", "b"]), Some("a de b".to_owned()));
    }
}
