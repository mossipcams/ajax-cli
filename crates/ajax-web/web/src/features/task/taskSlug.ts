export function slugifyTaskTitle(title: string): string {
  let slug = "";
  let previousWasDash = false;

  for (const character of title.toLowerCase()) {
    if (/[a-z0-9]/.test(character)) {
      slug += character;
      previousWasDash = false;
    } else if (!previousWasDash && slug.length > 0) {
      slug += "-";
      previousWasDash = true;
    }
  }

  while (slug.endsWith("-")) {
    slug = slug.slice(0, -1);
  }

  return slug.length > 0 ? slug : "task";
}

export function startTaskHandle(repo: string, title: string): string {
  return `${repo}/${slugifyTaskTitle(title)}`;
}
