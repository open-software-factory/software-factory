//! A rule id that comes from another linter is not a compile-time constant,
//! but [`Finding::rule`](crate::Finding::rule) is `&'static str` so that the
//! common case costs nothing. This interner bridges the two.
//!
//! The leak is bounded by the number of distinct rule ids a linter defines,
//! not by the size of the input: the same id is allocated once and every
//! later finding shares it. A linter with a few hundred rules leaks a few
//! kilobytes for the life of the process.
//!
//! Use it only for an identifier from a fixed vocabulary. Never intern text
//! taken from a file being checked, which has no such bound.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn table() -> &'static Mutex<HashSet<&'static str>> {
    static TABLE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Return a `&'static str` equal to `id`, allocating at most once per
/// distinct value.
///
/// If the lock is poisoned the entry is recovered rather than propagated:
/// a poisoned interner still holds valid strings, and a lint run must not
/// fail because an unrelated thread panicked.
#[must_use]
pub fn intern(id: &str) -> &'static str {
    let mut set = match table().lock() {
        Ok(set) => set,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(found) = set.get(id) {
        return found;
    }
    let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
    set.insert(leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::intern;

    #[test]
    fn the_same_id_is_allocated_once() {
        let first = intern("AS-001");
        let second = intern(&String::from("AS-001"));
        assert_eq!(first, "AS-001");
        assert!(
            std::ptr::eq(first, second),
            "a repeated id must reuse the first allocation"
        );
    }

    #[test]
    fn different_ids_stay_distinct() {
        assert_ne!(intern("AS-002"), intern("AS-003"));
    }
}
