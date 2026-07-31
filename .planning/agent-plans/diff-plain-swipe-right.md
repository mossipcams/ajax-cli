# Plain swipe-right opens Diff Review

Mode: Behavior Change.
Status: in progress.

## Product decision

User chose **plain swipe-right** (not long-press) to open Diff on task detail.
Swipe-left does not navigate. Diff Review: plain swipe-left returns.

Also keep `onOpenDiff` / `onBack` in refs so cockpit polls do not remount listeners.

Ships on top of the #718 revert branch (#719).

## Delegation decision

`Delegation decision: not delegated because` focused gesture behavior with a
clear product choice; smaller than a packet round-trip.

## Checklist

- [ ] TaskDetail / DiffReview / tests / e2e / architecture
- [ ] mobile-webkit smoke
- [ ] Push to #719
