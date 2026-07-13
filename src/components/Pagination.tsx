import { Button } from "./ui";

/** Numbered pagination: « ‹ [p−1][p][p+1] … [N−2][N−1][N] › » — the
 * immediate neighborhood plus the tail, with first/last jumps. Clusters
 * are deduplicated and gaps marked with an ellipsis. */
export function Pagination(props: {
  page: number;
  pageCount: number;
  onPage: (page: number) => void;
}) {
  const { page, pageCount, onPage } = props;
  if (pageCount <= 1) return null;

  const wanted = new Set<number>();
  for (const n of [page - 1, page, page + 1]) {
    if (n >= 0 && n < pageCount) wanted.add(n);
  }
  for (const n of [pageCount - 3, pageCount - 2, pageCount - 1]) {
    if (n >= 0 && n < pageCount) wanted.add(n);
  }
  const numbers = [...wanted].sort((a, b) => a - b);

  const chip = (n: number) => (
    <button
      key={n}
      type="button"
      onClick={() => onPage(n)}
      aria-current={n === page ? "page" : undefined}
      className={`min-w-[30px] rounded-control border px-2 py-1 text-xs transition ${
        n === page
          ? "border-transparent bg-accent font-semibold text-ink"
          : "border-border-subtle text-ink-secondary hover:border-gold/60"
      }`}
    >
      {(n + 1).toLocaleString()}
    </button>
  );

  const items: React.ReactNode[] = [];
  let prev: number | null = null;
  for (const n of numbers) {
    if (prev !== null && n - prev > 1) {
      items.push(
        <span key={`gap-${n}`} className="px-1 text-xs text-ink-muted">
          …
        </span>
      );
    }
    items.push(chip(n));
    prev = n;
  }

  return (
    <nav className="flex flex-wrap items-center gap-1.5" aria-label="Pagination">
      <Button variant="quiet" disabled={page === 0} onClick={() => onPage(0)}>
        « First
      </Button>
      <Button
        variant="quiet"
        disabled={page === 0}
        onClick={() => onPage(page - 1)}
      >
        ‹
      </Button>
      {items}
      <Button
        variant="quiet"
        disabled={page >= pageCount - 1}
        onClick={() => onPage(page + 1)}
      >
        ›
      </Button>
      <Button
        variant="quiet"
        disabled={page >= pageCount - 1}
        onClick={() => onPage(pageCount - 1)}
      >
        Last »
      </Button>
    </nav>
  );
}
