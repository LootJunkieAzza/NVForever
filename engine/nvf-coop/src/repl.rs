//! The reliable stream's item framing ([`crate::rel`] carries the items): an item is a kind
//! byte, then its records, little-endian. Only the kinds are here; the records themselves
//! (changes to a reference's components, digests of what a guest holds) come with the part
//! that shares the world, which registers its checks per kind with the host's guard
//! ([`crate::guard::Guard::add_rule`]).

/// Reliable item kinds. The host refuses any kind past
/// [`crate::guard::ITEM_KIND_MAX`] in a guest's items.
pub mod kind {
    /// Host to guest: changes to references.
    pub const CHANGES: u8 = 1;
    /// Guest to host: what the guest holds of references.
    pub const DIGEST: u8 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_are_ones_the_guard_lets_through() {
        for k in [kind::CHANGES, kind::DIGEST] {
            assert!(k <= crate::guard::ITEM_KIND_MAX);
        }
        assert_ne!(kind::CHANGES, kind::DIGEST);
    }
}
