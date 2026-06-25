<script lang="ts">
  import type { BrowserCockpitView } from "../types";
  import { filterByProject, sortCards, statusMeta } from "../state";
  import TaskCard from "./TaskCard.svelte";

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
            <button
              type="button"
              class="task-row tone-{meta.tone}"
              data-handle={card.qualified_handle}
              onclick={() => onOpenTask?.(card.qualified_handle)}
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
          {/each}
        </div>
      </section>
    {/each}
  </section>
{/if}

{#if visibleCount === 0}
  <p class="empty">{selectedProject ? `No tasks in ${selectedProject}` : "All quiet"}</p>
{/if}
