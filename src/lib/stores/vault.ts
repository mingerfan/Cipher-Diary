import { derived, writable } from 'svelte/store';
import type { Entry } from '../types';

export const unlocked = writable(false);
export const entries = writable<Entry[]>([]);
export const activeEntryId = writable<string | null>(null);
export const searchTerm = writable('');
export const lastSaved = writable<string | null>(null);
export const statusMessage = writable<string | null>(null);
export const vaultRoot = writable<string | null>(null);

export const filteredEntries = derived([entries, searchTerm], ([items, term]) => {
  if (!term.trim()) {
    return [...items].sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  }
  const needle = term.toLowerCase();
  return items
    .filter((entry) =>
      entry.title.toLowerCase().includes(needle) || entry.content.toLowerCase().includes(needle)
    )
    .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
});

export const activeEntry = derived([entries, activeEntryId], ([items, id]) => {
  if (!id) return null;
  return items.find((entry) => entry.id === id) ?? null;
});
