// Where a new issue goes (ISS-005, PRJ-012, PRJ-061): the projects the person may create issues in,
// in the picker's order, and the one chosen first.
import { api } from '#lib/api/client.ts';
import type { Project } from '#lib/api/schema.gen.ts';
import { can } from '#lib/session/index.ts';

/** Projects one request brings; more are found by typing. */
const LIMIT = 100;

/** Active projects matching `text` where the person may create issues. */
export async function creatableProjects(text = '', signal?: AbortSignal): Promise<Project[]> {
  const page = await api.get('/projects', {
    query: { section: 'active', q: text.trim() || undefined, limit: LIMIT },
    signal,
  });
  return page.items.filter(
    (project) => project.archived_at === null && can('issue.create', project.id),
  );
}

/** The picker's order: the context project, favorites, then the rest by name. */
export function orderProjects<P extends Pick<Project, 'id' | 'key' | 'name' | 'favorite'>>(
  projects: readonly P[],
  first?: string,
): P[] {
  const rank = (project: P): number => (project.key === first ? 0 : project.favorite ? 1 : 2);
  return [...projects].sort(
    (a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name) || a.id - b.id,
  );
}

/**
 * The project a new issue starts in: the entry point's if allowed, the only allowed one, the
 * person's last one if still allowed, or the first of the picker.
 */
export function defaultProject<P extends Pick<Project, 'id' | 'key' | 'name' | 'favorite'>>(
  projects: readonly P[],
  given: string | undefined,
  last: number | null,
): P | undefined {
  const ordered = orderProjects(projects, given);
  return (
    ordered.find((project) => project.key === given) ??
    (ordered.length === 1 ? ordered[0] : undefined) ??
    ordered.find((project) => project.id === last) ??
    ordered[0]
  );
}
