# Product

## Register

product

## Platform

web

## Users

Operators running several AI coding harnesses in parallel — Claude Code, Codex, Cursor, OpenCode — across multiple repos on their own machine. They hold more than one vendor subscription because subsidised capacity is siloed per vendor and does not transfer, and they want all of it working at once. They juggle Git worktrees, durable tmux sessions, and agent CLIs, and open Cockpit when they need to place the next task or find what is ready to ship.

## Product Purpose

Ajax keeps every subscription busy. It runs isolated tasks across every harness the operator owns, tracks where quota headroom actually is, places new work where there is room, and surfaces what is ready to review and ship. Success means extracting the full value of the plans already being paid for, while nothing gets lost, stuck, or shipped unreviewed. Capacity and throughput are the primary win; control and attention are how it stays trustworthy.

## Positioning

The scheduler for your subsidised agent capacity: every vendor, one queue, the host does the work.

## Brand Personality

Fast, decisive, under-control. The interface should feel like an operator console—immediate status, clear next action, no ceremony. Voice stays direct and operational; emotion is composure under load, not playful delight.

## Anti-references

- Generic SaaS dashboards: card grids, metric strips, soft purple/indigo chrome.
- Overbuilt IDE shells: too many panels and tabs fighting the terminal.
- Single-vendor cockpits: anything that treats one harness as the first-class citizen and the rest as afterthoughts.
- Agent frameworks: anything that reimplements the agent loop instead of driving the vendor's own CLI.

## Design Principles

1. **Every harness is a peer** — adding one is configuration, not a release. No vendor is first-class.
2. **Quota is the scarce resource** — surface headroom across vendors and place work against it.
3. **Ajax never calls models directly** — it drives vendor CLIs and observes them. Direct model access forfeits subsidised pricing and is out of scope by design.
4. **The host owns the truth** — Git and tmux are authoritative; Ajax reconciles against them and never invents state.
5. **Terminal is the work surface** — chrome supports the task; it never competes with the pane.
6. **Status and next safe action beat decoration** — every screen should answer what is happening and what to do.

## Standing Assumption

This positioning depends on vendor subscriptions being priced below the API rate for equivalent usage. That is a land-grab phase, not a law. If subscription pricing normalises toward cost recovery, the multi-vendor premise weakens and this document needs rewriting. Re-check quarterly.

## Accessibility & Inclusion

No formal accessibility requirements for now.
