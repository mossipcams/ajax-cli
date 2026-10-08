export function autoGrow(node: HTMLTextAreaElement, shrank: boolean) {
  if (shrank) node.style.height = "auto";
  else if (node.scrollHeight <= node.clientHeight) return;
  node.style.height = `${node.scrollHeight}px`;
}
