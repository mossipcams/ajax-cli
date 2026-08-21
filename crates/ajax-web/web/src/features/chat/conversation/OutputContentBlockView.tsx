import type { OutputContentBlock } from "@/shared/lib/liveSessionOutputContent";
import {
  embeddedResourceLabel,
  imageSource,
  resourceLabel,
} from "@/shared/lib/liveSessionOutputContent";

export default function OutputContentBlockView({ block }: { block: OutputContentBlock }) {
  if (block.type === "image") {
    const src = imageSource(block);
    if (!src) return null;
    return (
      <figure className="session-output-image" data-testid="session-output-image">
        <img src={src} alt="" />
      </figure>
    );
  }

  if (block.type === "resource_link") {
    const label = resourceLabel(block);
    return (
      <p className="session-output-resource" data-testid="session-output-resource-link">
        <span className="session-output-resource-name">{label}</span>
        <span className="session-output-resource-uri">{block.uri}</span>
      </p>
    );
  }

  const label = embeddedResourceLabel(block);
  return (
    <figure className="session-output-resource" data-testid="session-output-resource">
      <figcaption className="session-output-resource-name">{label}</figcaption>
      {block.text ? (
        <pre className="session-output-resource-text">{block.text}</pre>
      ) : (
        <span className="session-output-resource-uri">{block.uri}</span>
      )}
    </figure>
  );
}
