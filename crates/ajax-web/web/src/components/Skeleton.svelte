<script lang="ts">
  // Shimmer placeholder shown while a projection is loading. Purely decorative:
  // it never carries data, so it is hidden from assistive tech.
  interface Props {
    rows?: number;
    testid?: string;
  }

  let { rows = 4, testid }: Props = $props();
</script>

<div class="skeleton" data-testid={testid} aria-hidden="true">
  {#each Array.from({ length: rows }) as _, index (index)}
    <div class="skeleton-row"></div>
  {/each}
</div>

<style>
  .skeleton {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-4);
  }

  .skeleton-row {
    height: 56px;
    border-radius: var(--radius-lg);
    background: linear-gradient(
      100deg,
      var(--paper-tint) 30%,
      var(--paper-raised) 50%,
      var(--paper-tint) 70%
    );
    background-size: 220% 100%;
    animation: skeleton-sweep 1.4s var(--ease) infinite;
  }

  @keyframes skeleton-sweep {
    0% { background-position: 180% 0; }
    100% { background-position: -80% 0; }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton-row {
      animation: none;
      background: var(--paper-tint);
    }
  }
</style>
