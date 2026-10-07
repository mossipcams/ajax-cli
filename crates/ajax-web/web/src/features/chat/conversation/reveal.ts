export function settledText(text: string): string {
  const cut = text.lastIndexOf("\n\n");
  if (cut < 0) return "";
  const head = text.slice(0, cut);
  if ((head.match(/```/g) ?? []).length % 2 === 0) return head;
  return head.slice(0, head.lastIndexOf("```")).trimEnd();
}
