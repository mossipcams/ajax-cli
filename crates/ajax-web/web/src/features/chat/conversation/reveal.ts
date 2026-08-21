/** Complete paragraphs only. A partial sentence arriving word by word is the
 * protocol leaking into the conversation, so a live answer is cut back to its
 * last paragraph break — and never inside a fence, where the break is content. */
export function settledText(text: string): string {
  const cut = text.lastIndexOf("\n\n");
  if (cut < 0) return "";
  const head = text.slice(0, cut);
  if ((head.match(/```/g) ?? []).length % 2 === 0) return head;
  return head.slice(0, head.lastIndexOf("```")).trimEnd();
}
