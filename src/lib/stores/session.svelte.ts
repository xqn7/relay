import { api, type Account } from "../api";

class SessionStore {
  account = $state<Account | null>(null);
  /** False until the remembered sign-in has been checked, so no screen flashes. */
  ready = $state(false);
}

export const session = new SessionStore();

export async function restoreSession(): Promise<void> {
  try {
    session.account = await api.authState();
  } finally {
    session.ready = true;
  }
}
