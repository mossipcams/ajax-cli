# Product

## Register

product

## Platform

web

## Users

Operators running several AI coding harnesses in parallel — Claude Code, Codex, Cursor, OpenCode — across multiple repos on their own machine. They hold more than one vendor subscription because subsidised capacity is siloed per vendor and does not transfer, and they want all of it working at once. They juggle Git worktrees, durable tmux sessions, and agent CLIs. Most of the time they are not at the desk: they place work, step away, and come back to it from a phone.

## Product Purpose

Ajax keeps every subscription busy by keeping the operator reachable. It runs isolated tasks across every harness the operator owns, tracks where quota headroom actually is, places new work where there is room, and surfaces what is ready to review and ship — all of it from a phone as readily as from the desk. Capacity is wasted whenever the operator is the bottleneck, so utilisation and reach are the same problem. Success means extracting the full value of the plans already being paid for, while nothing gets lost, stuck, or shipped unreviewed.

## Positioning

A mobile orchestrator for your whole agent fleet: direct any agent with quick actions from Safari on the phone, while the host does the work.

## Brand Personality

Fast, decisive, under-control. The interface should feel like an operator console—immediate status, clear next action, no ceremony. Voice stays direct and operational; emotion is composure under load, not playful delight.

## Anti-references

- Generic SaaS dashboards: card grids, metric strips, soft purple/indigo chrome.
- Overbuilt IDE shells: too many panels and tabs fighting the work surface.
- Read-only dashboards: watching a fleet you cannot direct from the same screen.
- Single-vendor cockpits: anything that treats one harness as the first-class citizen and the rest as afterthoughts.
- Agent frameworks: anything that reimplements the agent loop instead of driving the vendor's own CLI.

## Design Principles

1. **Quick actions direct the agent** — the structured session is the work surface: prompt, stop, approve, attach context, act on a diff. The terminal stays one tap away as the escape hatch for anything the session cannot express, never as the thing the operator must fall back to by default.
2. **Mobile-reachable without diluting host authority** — phone access is first-class; task truth stays on the host. Reach is not a convenience: capacity sits idle whenever the operator cannot act, so the phone loop is how subscriptions get used.
3. **Every harness is a peer, over one protocol** — Ajax speaks ACP rather than per-vendor hooks and pane scraping. Adding a conforming agent is configuration, not a release. No vendor is first-class.
4. **Quota is the scarce resource** — surface headroom across vendors and place work against it.
5. **Ajax never calls models directly** — it drives vendor CLIs and observes them. Direct model access forfeits subsidised pricing and is out of scope by design.
6. **Status and next safe action beat decoration** — every screen should answer what is happening and what to do, and prefer decisive operator intents over exploratory chrome.

## Standing Assumption

This positioning depends on vendor subscriptions being priced below the API rate for equivalent usage. That is a land-grab phase, not a law. If subscription pricing normalises toward cost recovery, the multi-vendor premise weakens and this document needs rewriting. Re-check quarterly.

## Accessibility & Inclusion

No formal accessibility requirements for now.
