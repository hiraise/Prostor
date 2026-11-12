// Reading the emails the server sent, from Mailpit (deploy/compose.dev.yaml, http://127.0.0.1:8025).
import { expect, type APIRequestContext } from '@playwright/test';

const MAILPIT = process.env.TRACKFLOW_E2E_MAILPIT ?? 'http://127.0.0.1:8025';

interface Summary {
  ID: string;
  Subject: string;
}

/** The newest email to `address` whose subject contains `subject`, waiting for it to arrive. */
export async function lastMail(
  request: APIRequestContext,
  address: string,
  subject: string,
): Promise<{ subject: string; text: string }> {
  let found: Summary | undefined;
  await expect
    .poll(
      async () => {
        const answer = await request.get(`${MAILPIT}/api/v1/search`, {
          params: { query: `to:"${address}"`, limit: '10' },
        });
        const body = (await answer.json()) as { messages?: Summary[] };
        found = body.messages?.find((message) => message.Subject.includes(subject));
        return found !== undefined;
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  const message = await request.get(`${MAILPIT}/api/v1/message/${found?.ID ?? ''}`);
  const body = (await message.json()) as { Subject: string; Text: string };
  return { subject: body.Subject, text: body.Text };
}

/** The address part of the link in an email: everything from the path on, secret included. */
export function linkPath(text: string, page: string): string {
  const match = new RegExp(`https?://[^\\s]+/${page}#[A-Za-z0-9_-]+`).exec(text);
  if (!match) throw new Error(`no ${page} link in: ${text}`);
  const url = new URL(match[0]);
  return url.pathname + url.hash;
}

/** An address nobody used before, so runs and projects never see each other's emails. */
export function freshAddress(name: string): string {
  return `${name}-${String(Date.now())}-${String(Math.floor(Math.random() * 1e6))}@example.com`;
}
