//! Mobile attention delivery.

use std::collections::BTreeSet;

pub fn new_attention_handles(
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
) -> Vec<String> {
    current.difference(previous).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::new_attention_handles;
    use std::collections::BTreeSet;

    #[test]
    fn attention_slice_detects_new_attention_handles() {
        let previous = BTreeSet::from(["web/a".to_string(), "web/c".to_string()]);
        let current = BTreeSet::from([
            "web/c".to_string(),
            "web/b".to_string(),
            "web/d".to_string(),
        ]);

        assert_eq!(
            new_attention_handles(&previous, &current),
            vec!["web/b".to_string(), "web/d".to_string()]
        );
    }
}
