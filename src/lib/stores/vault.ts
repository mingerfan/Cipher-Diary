import { derived, writable } from 'svelte/store';
import type { EntryDetail, EntrySummary, TextEncryption } from '../types';

export const unlocked = writable(false);
export const entries = writable<EntrySummary[]>([]);
export const activeEntryId = writable<string | null>(null);
export const searchTerm = writable('');
export const lastSaved = writable<string | null>(null);
export const statusMessage = writable<string | null>(null);
export const activeEntryDetail = writable<EntryDetail | null>(null);
export const textEncryption = writable<TextEncryption>('aes256_gcm');
export const availableTextEncryptions = writable<TextEncryption[]>(['aes256_gcm']);

const VAULT_ROOT_KEY = 'diary:vault-root';

function createVaultRootStore() {
  const initial = typeof window === 'undefined' ? null : window.localStorage.getItem(VAULT_ROOT_KEY);
  const { subscribe, set: setInner, update: updateInner } = writable<string | null>(initial);

  return {
    subscribe,
    set(value: string | null) {
      if (typeof window !== 'undefined') {
        if (value) {
          window.localStorage.setItem(VAULT_ROOT_KEY, value);
        } else {
          window.localStorage.removeItem(VAULT_ROOT_KEY);
        }
      }
      setInner(value);
    },
    update(updater: (value: string | null) => string | null) {
      let next: string | null = null;
      updateInner((current) => {
        next = updater(current);
        return next;
      });
      if (typeof window !== 'undefined') {
        if (next) {
          window.localStorage.setItem(VAULT_ROOT_KEY, next);
        } else {
          window.localStorage.removeItem(VAULT_ROOT_KEY);
        }
      }
    }
  };
}

export const vaultRoot = createVaultRootStore();

export const filteredEntries = derived([entries, searchTerm], ([items, term]) => {
  if (!term.trim()) {
    return [...items].sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  }
  const needle = term.toLowerCase();
  return items
    .filter((entry) => entry.title.toLowerCase().includes(needle))
    .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
});

export const activeEntry = derived([entries, activeEntryId], ([items, id]) => {
  if (!id) return null;
  return items.find((entry) => entry.id === id) ?? null;
});
