import { api, type Account } from "../api";

class SessionStore {
  account = $state<Account | null>(null);
  /** False until the remembered sign-in has been checked, so no screen flashes. */
  ready = $state(false);
  /** The remembered account has an app lock that hasn't been opened yet. */
  locked = $state(false);
}

export const session = new SessionStore();

export async function restoreSession(): Promise<void> {
  try {
    session.account = await api.authState();
    if (session.account) session.locked = (await api.lockStatus().catch(() => null))?.locked ?? false;
  } finally {
    session.ready = true;
  }
}
