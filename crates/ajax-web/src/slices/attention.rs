//! Mobile attention delivery.

use std::collections::{BTreeSet, HashMap};

const ABSENT_POLLS_BEFORE_RENOTIFY: u32 = 2;

pub fn new_attention_handles(
    previous: &BTreeSet<String>,
    current: &BTreeSet<String>,
) -> Vec<String> {
    current.difference(previous).cloned().collect()
}

/// Tracks inbox attention across polls so push is sent once per genuine entry.
pub struct AttentionNotifier {
    previous: BTreeSet<String>,
    notified: BTreeSet<String>,
    absent_counts: HashMap<String, u32>,
}

impl AttentionNotifier {
    pub fn seeded_with(current: BTreeSet<String>) -> Self {
        Self {
            previous: current.clone(),
            notified: current,
            absent_counts: HashMap::new(),
        }
    }

    pub fn poll(&mut self, current: BTreeSet<String>) -> Vec<String> {
        for handle in self
            .notified
            .difference(&current)
            .cloned()
            .collect::<Vec<_>>()
        {
            let count = self.absent_counts.entry(handle.clone()).or_insert(0);
            *count += 1;
            if *count >= ABSENT_POLLS_BEFORE_RENOTIFY {
                self.notified.remove(&handle);
                self.absent_counts.remove(&handle);
            }
        }
        for handle in &current {
            self.absent_counts.remove(handle);
        }

        let mut to_notify = Vec::new();
        for handle in new_attention_handles(&self.previous, &current) {
            if self.notified.insert(handle.clone()) {
                to_notify.push(handle);
            }
        }
        self.previous = current;
        to_notify
    }
}

#[cfg(test)]
mod tests {
    use super::{new_attention_handles, AttentionNotifier};
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

    #[test]
    fn notifier_skips_existing_inbox_on_boot() {
        let mut notifier = AttentionNotifier::seeded_with(BTreeSet::from(["web/a".to_string()]));
        assert!(notifier
            .poll(BTreeSet::from(["web/a".to_string()]))
            .is_empty());
    }

    #[test]
    fn notifier_sends_once_for_new_handle() {
        let mut notifier = AttentionNotifier::seeded_with(BTreeSet::new());
        assert_eq!(
            notifier.poll(BTreeSet::from(["web/a".to_string()])),
            vec!["web/a".to_string()]
        );
        assert!(notifier
            .poll(BTreeSet::from(["web/a".to_string()]))
            .is_empty());
    }

    #[test]
    fn notifier_ignores_single_poll_inbox_flap() {
        let mut notifier = AttentionNotifier::seeded_with(BTreeSet::from(["web/a".to_string()]));
        assert!(notifier.poll(BTreeSet::new()).is_empty());
        assert!(notifier
            .poll(BTreeSet::from(["web/a".to_string()]))
            .is_empty());
    }

    #[test]
    fn notifier_can_renotify_after_sustained_absence() {
        let mut notifier = AttentionNotifier::seeded_with(BTreeSet::from(["web/a".to_string()]));
        assert!(notifier.poll(BTreeSet::new()).is_empty());
        assert!(notifier.poll(BTreeSet::new()).is_empty());
        assert_eq!(
            notifier.poll(BTreeSet::from(["web/a".to_string()])),
            vec!["web/a".to_string()]
        );
    }
}
