import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { EntryDetail, EntrySummary, UnlockResponse } from './types';

export async function unlockVault(passphrase: string, directory?: string | null): Promise<UnlockResponse> {
  return invoke<UnlockResponse>('unlock_vault', { passphrase, directory: directory ?? undefined });
}

export async function lockVault(): Promise<void> {
  await invoke('lock_vault');
}

export async function fetchEntries(): Promise<EntrySummary[]> {
  return invoke<EntrySummary[]>('list_entries');
}

export async function loadVaultEntry(id: string): Promise<EntryDetail> {
  return invoke<EntryDetail>('load_entry', { id });
}

export async function createVaultEntry(title?: string, content?: string): Promise<EntryDetail> {
  return invoke<EntryDetail>('create_entry', { title, content });
}

export async function updateVaultEntry(entry: EntryDetail): Promise<EntryDetail> {
  return invoke<EntryDetail>('update_entry', { entry });
}

export async function deleteVaultEntry(id: string): Promise<void> {
  await invoke('delete_entry', { id });
}

export async function exportVaultToFile(): Promise<string> {
  return invoke<string>('export_plaintext_file');
}

export async function importVaultImage(path: string): Promise<string> {
  return invoke<string>('store_image', { path });
}

export async function pickVaultDirectory(): Promise<string | null> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: '选择日记存储文件夹'
  });

  if (Array.isArray(selection)) {
    return selection[0] ?? null;
  }

  return typeof selection === 'string' ? selection : null;
}

export async function pickImageFile(): Promise<string | null> {
  const selection = await open({
    multiple: false,
    directory: false,
    filters: [
      { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] }
    ],
    title: '选择要插入的图片'
  });

  if (Array.isArray(selection)) {
    return selection[0] ?? null;
  }

  return typeof selection === 'string' ? selection : null;
}
