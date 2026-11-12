// The admin mail settings (NTF-002, NTF-004, NTF-006, NTF-007; plan step 1.10 §5.1): the calls of
// the «Почта» screen of the administration area. Secret values are write-only: the field absent
// keeps the stored value, `null` clears it.
import { api } from '#lib/api/client.ts';
import type { MailSettings, MailSettingsChange, MailTestSend } from '#lib/api/schema.gen.ts';

/** The mail settings of the instance; no secret value comes back, only that one is stored. */
export function getMailSettings(signal?: AbortSignal): Promise<MailSettings> {
  return api.get('/admin/mail', { signal });
}

/** Saves a change of the settings; the answer is how they stand now. */
export function saveMailSettings(change: MailSettingsChange): Promise<MailSettings> {
  return api.put('/admin/mail', { body: change });
}

/**
 * Sends a test message to any address through the saved transport (NTF-007). The transport's
 * complaint, if any, comes back in full — the promise rejects with the server's words.
 */
export function sendMailTest(to: string): Promise<MailTestSend> {
  return api.post('/admin/mail/test', { body: { to } });
}
