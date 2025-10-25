export interface Entry {
  id: string;
  title: string;
  content: string;
  created_at: string;
  updated_at: string;
}

export interface UnlockResponse {
  entries: Entry[];
  created: boolean;
  last_saved?: string | null;
  vault_root: string;
}
