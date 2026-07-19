interface Props {
  rows?: number;
  testid?: string;
}

export default function Skeleton({ rows = 4, testid }: Props) {
  return (
    <div className="skeleton" data-testid={testid} aria-hidden="true">
      {Array.from({ length: rows }).map((_, index) => (
        <div className="skeleton-row" data-testid="skeleton-row" key={index} />
      ))}
    </div>
  );
}
