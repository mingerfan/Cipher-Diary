export interface EntrySummary {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  folder?: string | null;
}

export interface EntryDetail extends EntrySummary {
  content: string;
}

export interface UnlockResponse {
  entries: EntrySummary[];
  created: boolean;
  last_saved?: string | null;
  vault_root: string;
}
