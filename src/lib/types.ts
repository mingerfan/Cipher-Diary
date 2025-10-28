export type TextEncryption = 'aes256_gcm';

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
  text_encryption: TextEncryption;
  available_text_encryptions: TextEncryption[];
}
