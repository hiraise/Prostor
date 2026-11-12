// Comments and files of issues in the data store (architecture.md §15). The issue page puts them
// in from its one request (`GET /issues/{id}/page`); the loaders are for live refetches of one
// comment or file.
import { api } from '#lib/api/client.ts';
import type { Attachment, Comment } from '#lib/api/schema.gen.ts';
import { store } from '#lib/store/index.ts';

declare module '#lib/store/index.ts' {
  interface EntityKinds {
    comment: Comment;
    attachment: Attachment;
  }
}

store.defineKind('comment', {
  async load(ids) {
    const answers = await Promise.all(
      ids.map((id) =>
        api
          .get('/comments/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return answers.filter((comment): comment is Comment => comment !== null);
  },
});

store.defineKind('attachment', {
  async load(ids) {
    const answers = await Promise.all(
      ids.map((id) =>
        api
          .get('/attachments/{id}', { path: { id }, accept: [404] })
          .then((answer) => ('id' in answer ? answer : null)),
      ),
    );
    return answers.filter((file): file is Attachment => file !== null);
  },
});
