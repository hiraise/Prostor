// A person's picture (ACC-017): uploaded, taken from Gravatar or removed. The new address goes
// into the stored account and, for the signed-in person, into the session, so the account menu
// shows it at once.
import { api } from '#lib/api/client.ts';
import type { Account } from '#lib/api/schema.gen.ts';
import { uploadFile } from '#lib/api/upload.ts';
import { keepAccount } from '#lib/features/access/index.ts';
import { currentUser, updateCurrentUser } from '#lib/session/index.ts';

function show(account: Account, url: string | null): void {
  keepAccount({ ...account, avatar_url: url }, 'full');
  const user = currentUser();
  if (user?.id === account.id) updateCurrentUser({ ...user, avatar_url: url });
}

/** Sends a picture and makes it the account's avatar. */
export async function uploadAvatar(account: Account, file: File): Promise<string | null> {
  const upload = await uploadFile(file);
  const picture = await api.put('/users/{id}/avatar', {
    path: { id: account.id },
    body: { upload: upload.id },
  });
  show(account, picture.url);
  return picture.url;
}

/** Takes the picture Gravatar has for the account's email. */
export async function importGravatar(account: Account): Promise<string | null> {
  const picture = await api.post('/users/{id}/avatar/gravatar', { path: { id: account.id } });
  show(account, picture.url);
  return picture.url;
}

/** Removes the picture: the initials are shown again. */
export async function removeAvatar(account: Account): Promise<void> {
  await api.delete('/users/{id}/avatar', { path: { id: account.id } });
  show(account, null);
}
