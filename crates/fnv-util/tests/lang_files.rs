//! Brace places in a translated line have to be the same names as in the English.

use std::collections::BTreeSet;

fn places(s: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.insert(name);
        }
        rest = &after[close + 1..];
    }
    out
}

#[test]
fn places_are_names_in_braces() {
    assert_eq!(places("{n} of {len} right"), BTreeSet::from(["n", "len"]));
    assert_eq!(places("no places {} { x }"), BTreeSet::new());
}
