//! Inventories: item stacks keyed by base form.
//!
//! Stacks are plain counts per base item. Per-instance extra data (condition,
//! ownership, weapon mods, equipped state) is not modelled yet; it becomes a list of
//! extra-data entries per stack when Layer 6 needs it (docs/design/layer4.md §2.5).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::form::FormId;
use crate::forms::FormStore;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Inventory {
    items: BTreeMap<FormId, i32>,
}

/// Total weight and value, with the items whose weight or value is not known yet.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub weight: f64,
    pub value: i64,
    pub unknown_weight: Vec<FormId>,
    pub unknown_value: Vec<FormId>,
}

impl Inventory {
    pub fn new() -> Inventory {
        Inventory::default()
    }

    /// Add `n` (> 0) of `item`; returns the new count.
    pub fn add(&mut self, item: FormId, n: i32) -> i32 {
        debug_assert!(n > 0);
        let c = self.items.entry(item).or_default();
        *c = c.saturating_add(n);
        *c
    }

    /// Remove up to `n` of `item`; returns how many were removed.
    pub fn remove(&mut self, item: FormId, n: i32) -> i32 {
        let Some(c) = self.items.get_mut(&item) else { return 0 };
        let taken = (*c).min(n.max(0));
        *c -= taken;
        if *c == 0 {
            self.items.remove(&item);
        }
        taken
    }

    pub fn count(&self, item: FormId) -> i32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Stacks in FormID order.
    pub fn iter(&self) -> impl Iterator<Item = (FormId, i32)> + '_ {
        self.items.iter().map(|(&k, &v)| (k, v))
    }

    pub fn totals(&self, forms: &FormStore) -> Totals {
        let mut t = Totals::default();
        for (id, n) in self.iter() {
            match forms.item(id) {
                Some(i) => {
                    match i.weight {
                        Some(w) => t.weight += w as f64 * n as f64,
                        None => t.unknown_weight.push(id),
                    }
                    match i.value {
                        Some(v) => t.value += v as i64 * n as i64,
                        None => t.unknown_value.push(id),
                    }
                }
                None => {
                    t.unknown_weight.push(id);
                    t.unknown_value.push(id);
                }
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn add_remove_count() {
        let mut inv = Inventory::new();
        let a = FormId(0x10);
        assert_eq!(inv.add(a, 3), 3);
        assert_eq!(inv.add(a, 2), 5);
        assert_eq!(inv.remove(a, 4), 4);
        assert_eq!(inv.count(a), 1);
        assert_eq!(inv.remove(a, 9), 1);
        assert!(inv.is_empty());
        assert_eq!(inv.remove(a, 1), 0);
        assert_eq!(inv.remove(FormId(0x11), -3), 0);
        let j = serde_json::to_string(&{
            let mut i = Inventory::new();
            i.add(FormId(0x2), 1);
            i.add(FormId(0x1), 7);
            i
        })
        .unwrap();
        assert_eq!(j, r#"{"0x00000001":7,"0x00000002":1}"#);
        let back: Inventory = serde_json::from_str(&j).unwrap();
        assert_eq!(back.count(FormId(0x1)), 7);
    }

    proptest! {
        /// Counts never go negative and the total is conserved by add/remove.
        #[test]
        fn conservation(ops in proptest::collection::vec((0u32..4, 1i32..50, any::<bool>()), 0..200)) {
            let mut inv = Inventory::new();
            let mut expect = [0i64; 4];
            for (item, n, add) in ops {
                if add {
                    inv.add(FormId(item), n);
                    expect[item as usize] += n as i64;
                } else {
                    let got = inv.remove(FormId(item), n);
                    prop_assert!(got >= 0 && got <= n);
                    expect[item as usize] -= got as i64;
                }
                for (i, e) in expect.iter().enumerate() {
                    prop_assert_eq!(inv.count(FormId(i as u32)) as i64, *e);
                    prop_assert!(*e >= 0);
                }
            }
        }
    }
}
