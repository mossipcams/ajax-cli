<script lang="ts">
  import type { BrowserCockpitView } from "../types";
  import { filterByProject, sortCards, statusMeta } from "../state";
  import TaskCard from "./TaskCard.svelte";
  import ActionBar from "./ActionBar.svelte";
  import { swipeReveal } from "../gestures/swipeRevealAction";
  import { SWIPE_REVEAL_WIDTH } from "../gestures/swipeReveal";

  // Per-row reveal offset, keyed by handle. A swipe slides the row left to
  // expose its first action; tapping an open row closes it instead of opening.
  let offsets = $state<Record<string, number>>({});
  function setOffset(handle: string, offset: number) {
    offsets = { ...offsets, [handle]: offset };
  }
  function rowTap(handle: string) {
    if ((offsets[handle] ?? 0) > 0) {
      setOffset(handle, 0);
      return;
    }
    onOpenTask?.(handle);
  }

  interface Props {
    cockpit: BrowserCockpitView;
    selectedProject?: string | null;
    onSelectProject?: (project: string | null) => void;
    onOpenTask?: (handle: string) => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onMutated?: () => void;
  }

  let {
    cockpit,
    selectedProject = null,
    onSelectProject,
    onOpenTask,
    onCockpit,
    onResult,
    onMutated,
  }: Props = $props();

  let cardsByHandle = $derived(
    new Map(cockpit.cards.map((card) => [card.qualified_handle, card])),
  );

  // Project pills: every repo seen on a card plus every configured repo.
  let projects = $derived(
    [
      ...new Set([
        ...cockpit.cards.map((card) => card.repo),
        ...(cockpit.repos?.repos ?? []).map((repo) => repo.name),
      ]),
    ].sort(),
  );

  let inboxItems = $derived(
    (cockpit.inbox?.items ?? [])
      .slice()
      .sort((a, b) => (a.severity ?? 999) - (b.severity ?? 999))
      .map((item) => ({ item, card: cardsByHandle.get(item.task_handle) }))
      .filter(
        (entry): entry is { item: typeof entry.item; card: NonNullable<typeof entry.card> } =>
          entry.card != null &&
          (!selectedProject || entry.card.repo === selectedProject),
      ),
  );

  let inboxHandles = $derived(new Set(inboxItems.map((entry) => entry.card.qualified_handle)));

  // Calm list: visible cards not already in the inbox, grouped by repo.
  let groups = $derived(
    (() => {
      const visible = filterByProject(cockpit.cards, selectedProject).filter(
        (card) => !inboxHandles.has(card.qualified_handle),
      );
      const byRepo = new Map<string, typeof visible>();
      for (const card of visible) {
        if (!byRepo.has(card.repo)) byRepo.set(card.repo, []);
        byRepo.get(card.repo)!.push(card);
      }
      return [...byRepo.keys()]
        .sort()
        .map((repo) => ({ repo, cards: sortCards(byRepo.get(repo)!) }));
    })(),
  );

  let calmCount = $derived(groups.reduce((sum, group) => sum + group.cards.length, 0));
  let showRepoTitles = $derived(!selectedProject && groups.length > 1);
  let visibleCount = $derived(filterByProject(cockpit.cards, selectedProject).length);
</script>

{#if projects.length}
  <nav class="project-nav" aria-label="Projects">
    <span class="project-nav-label">Projects</span>
    <button
      type="button"
      class="project-pill"
      class:is-active={!selectedProject}
      onclick={() => onSelectProject?.(null)}
    >
      All
    </button>
    {#each projects as project (project)}
      <button
        type="button"
        class="project-pill"
        class:is-active={selectedProject === project}
        onclick={() => onSelectProject?.(project)}
      >
        {project}
      </button>
    {/each}
  </nav>
{/if}

{#if inboxItems.length}
  <section class="group inbox" aria-live="polite">
    <div class="section-head attention">
      <span class="section-head-title">Needs you</span>
      <span class="section-head-count">{inboxItems.length}</span>
    </div>
    <div class="inbox-list">
      {#each inboxItems as entry (entry.card.qualified_handle)}
        <TaskCard
          card={entry.card}
          severity={entry.item.severity}
          {onOpenTask}
          {onCockpit}
          {onResult}
          {onMutated}
        />
      {/each}
    </div>
  </section>
{/if}

{#if calmCount}
  <section class="group tasks" aria-live="polite">
    <div class="section-head">
      <span class="section-head-title">{selectedProject ?? "Tasks"}</span>
      <span class="section-head-count">{calmCount}</span>
    </div>
    {#each groups as group (group.repo)}
      <section class="task-group">
        {#if showRepoTitles}
          <div class="task-group-title">{group.repo}</div>
        {/if}
        <div class="task-list">
          {#each group.cards as card (card.qualified_handle)}
            {@const meta = statusMeta(card.status)}
            {@const revealAction = card.actions.find((action) => action.action !== "resume")}
            <div class="task-row-wrap" data-handle={card.qualified_handle}>
              {#if revealAction}
                <div class="task-row-reveal" style="width: {SWIPE_REVEAL_WIDTH}px">
                  <ActionBar
                    actions={[revealAction]}
                    handle={card.qualified_handle}
                    {onCockpit}
                    {onResult}
                    {onMutated}
                  />
                </div>
              {/if}
              <button
                type="button"
                class="task-row tone-{meta.tone}"
                class:is-revealed={(offsets[card.qualified_handle] ?? 0) > 0}
                data-handle={card.qualified_handle}
                style="transform: translateX(-{offsets[card.qualified_handle] ?? 0}px)"
                onclick={() => rowTap(card.qualified_handle)}
                use:swipeReveal={revealAction
                  ? {
                      onOffset: (offset) => setOffset(card.qualified_handle, offset),
                      onOpenChange: () => {},
                    }
                  : {}}
              >
                <span class="status-dot tone-{meta.tone}" aria-hidden="true"></span>
                <div class="task-row-main">
                  <span class="task-row-handle">{card.qualified_handle}</span>
                  {#if card.status_explanation && card.status_explanation.toLowerCase() !== meta.label.toLowerCase()}
                    <span class="task-row-sub">{card.status_explanation}</span>
                  {/if}
                </div>
                <span class="task-row-status">{meta.label}</span>
                <span class="task-row-chevron">›</span>
              </button>
            </div>
          {/each}
        </div>
      </section>
    {/each}
  </section>
{/if}

{#if visibleCount === 0}
  <p class="empty">{selectedProject ? `No tasks in ${selectedProject}` : "All quiet"}</p>
{/if}

<style>
  /* PROJECT NAV ----------------------------------------------------------- */
  .project-nav {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 4px 0 18px;
    -ms-overflow-style: none;
    scrollbar-width: none;
  }

  .project-nav-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-faint);
    margin-right: 4px;
  }

  .project-pill {
    background: transparent;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    color: var(--ink-soft);
    font-size: 11.5px;
    font-weight: 500;
    letter-spacing: 0.04em;
    padding: 5px 12px;
    min-height: 28px;
    transition: background 140ms ease, border-color 140ms ease, color 140ms ease;
  }

  .project-pill:hover,
  .project-pill:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  .project-pill.is-active {
    background: var(--mustard);
    border-color: var(--mustard);
    color: #1c1714;
    font-weight: 600;
  }

  /* SECTION HEADS — small caps + count chip ------------------------------- */
  .section-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 26px 0 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--rule);
  }

  .group:first-of-type .section-head {
    margin-top: 6px;
  }

  .section-head::after {
    content: "";
    flex: 1 1 auto;
  }

  .section-head-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .section-head.attention .section-head-title {
    color: var(--mustard-bright);
  }

  .section-head-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 18px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--paper-high);
    color: var(--ink-soft);
    font-size: 11px;
    font-weight: 700;
    font-feature-settings: "tnum";
  }

  .section-head.attention .section-head-count {
    background: var(--mustard);
    color: #1c1714;
  }

  /* INBOX LIST — grid of attention cards (cards styled in TaskCard) -------- */
  .inbox-list {
    display: grid;
    gap: 10px;
  }

  /* CALM TASK LIST — light, glanceable rows -------------------------------- */
  .task-group + .task-group {
    margin-top: 16px;
  }

  .task-group-title {
    margin: 0 0 6px;
    padding-left: 2px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  .task-list {
    display: grid;
    border: 1px solid var(--rule);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--paper-tint);
    box-shadow: var(--elev-1);
  }

  /* Each row sits in a clipping wrapper so the swipe-revealed action stays
     hidden behind it until the row slides left. */
  .task-row-wrap {
    position: relative;
    overflow: hidden;
    border-top: 1px solid var(--rule);
  }

  .task-row-wrap:first-child {
    border-top: none;
  }

  .task-row-reveal {
    position: absolute;
    inset: 0 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-right: var(--space-2);
  }

  .task-row {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 56px;
    padding: var(--space-3) var(--space-4);
    background: var(--paper-tint);
    border: none;
    color: var(--ink);
    text-align: left;
    transition: background 120ms var(--ease), transform 220ms var(--ease-spring);
    touch-action: pan-y;
  }

  .task-row:hover,
  .task-row:focus-visible {
    background: var(--paper-raised);
    outline: none;
  }

  .task-row:active {
    background: var(--paper-high);
  }

  .task-row-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1 1 auto;
    min-width: 0;
  }

  .task-row-handle {
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ink);
  }

  .task-row-sub {
    font-size: 12px;
    color: var(--ink-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .task-row-status {
    flex: none;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--tone, var(--ink-muted));
  }

  .task-row-chevron {
    flex: none;
    font-size: 18px;
    line-height: 1;
    color: var(--ink-faint);
  }
</style>
