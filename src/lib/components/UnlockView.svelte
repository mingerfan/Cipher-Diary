<script lang="ts">
  
  import { get } from 'svelte/store';
  import { pickVaultDirectory, unlockVault } from '../api';
  import {
    activeEntryDetail,
    activeEntryId,
    entries,
    lastSaved,
    statusMessage,
    unlocked as unlockedStore,
    vaultRoot,
    availableTextEncryptions,
    textEncryption
  } from '../stores/vault';
  import type { TextEncryption, UnlockResponse } from '../types';

  let { unlocked } = $props<{ unlocked?: (payload: { created: boolean }) => void }>();

  let passphrase = $state('');
  let confirm = $state('');
  let requireConfirmation = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let selectedDirectory = $state<string | null>(get(vaultRoot));

  const ENCRYPTION_LABELS: Record<TextEncryption, string> = {
    aes256_gcm: 'AES-256-GCM',
    chacha20_poly1305: 'ChaCha20-Poly1305'
  };

  let selectedEncryption = $state<TextEncryption>(get(textEncryption));


  function toggleConfirmation(event: Event) {
    const target = event.target as HTMLInputElement;
    requireConfirmation = target.checked;
    if (!target.checked) {
      confirm = '';
    }
  }

  async function chooseDirectory() {
    try {
      const result = await pickVaultDirectory();
      if (result) {
        selectedDirectory = result;
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : '无法打开文件夹选择器';
      error = message;
    }
  }

  function clearDirectory() {
    selectedDirectory = null;
  }

  function handleEncryptionChange(event: Event) {
    const value = (event.target as HTMLSelectElement).value as TextEncryption;
    selectedEncryption = value;
    textEncryption.set(value);
  }

  async function handleSubmit(event: Event) {
    event.preventDefault();
    error = null;

    if (!passphrase.trim()) {
      error = '请输入密码短语';
      return;
    }

    if (requireConfirmation && passphrase !== confirm) {
      error = '两次输入的密码短语不一致';
      return;
    }

    busy = true;
    try {
  textEncryption.set(selectedEncryption);
  const response: UnlockResponse = await unlockVault(passphrase, selectedDirectory, selectedEncryption);
      entries.set(response.entries);
      lastSaved.set(response.last_saved ?? null);
      vaultRoot.set(response.vault_root);
  availableTextEncryptions.set(response.available_text_encryptions);
  textEncryption.set(response.text_encryption);
  selectedEncryption = response.text_encryption;
      activeEntryDetail.set(null);
      unlockedStore.set(true);
      activeEntryId.set(response.entries[0]?.id ?? null);
      statusMessage.set(response.created ? '新的日记库已创建' : '日记库已解锁');
      unlocked?.({ created: response.created });
      selectedDirectory = null;
    } catch (err) {
      const message = err instanceof Error ? err.message : '无法解锁日记库';
      error = message.includes('decryption failed') ? '密码短语错误，请重试' : message;
    } finally {
      busy = false;
      passphrase = '';
      confirm = '';
    }
  }
</script>

<div class="unlock-container">
  <div class="card glass">
    <h1>🔐 本地加密日记</h1>
    <p class="tagline">参照 DailyTxT 的单页体验，所有数据仅保存在本机。</p>

  <form onsubmit={handleSubmit}>
      <label for="passphrase">密码短语</label>
      <input
        id="passphrase"
        type="password"
        bind:value={passphrase}
        placeholder="请输入密码短语"
        autocomplete="current-password"
        required
      />

      <label class="confirm-toggle">
  <input type="checkbox" onchange={toggleConfirmation} />
        <span>首次使用？勾选后重复输入以防输入错误</span>
      </label>

      {#if requireConfirmation}
        <label for="confirm">确认密码短语</label>
        <input
          id="confirm"
          type="password"
          bind:value={confirm}
          placeholder="请再次输入"
          autocomplete="new-password"
          required
        />
      {/if}

      <label for="encryption">文本加密算法</label>
      <select
        id="encryption"
        bind:value={selectedEncryption}
        onchange={handleEncryptionChange}
        disabled={busy}
      >
        {#each $availableTextEncryptions as option}
          <option value={option}>{ENCRYPTION_LABELS[option] ?? option}</option>
        {/each}
      </select>

      {#if error}
        <div class="error">{error}</div>
      {/if}

      <div class="directory-picker">
        <p class="directory-label">存储文件夹（可选）</p>
        <p class="directory-hint">
          {#if selectedDirectory}
            {selectedDirectory}
          {:else}
            未选择时将保存在应用数据目录中
          {/if}
        </p>
        <div class="picker-actions">
          <button type="button" class="secondary" onclick={chooseDirectory} disabled={busy}>
            选择文件夹
          </button>
          {#if selectedDirectory}
            <button type="button" class="ghost" onclick={clearDirectory} disabled={busy}>
              使用默认位置
            </button>
          {/if}
        </div>
      </div>

      <button type="submit" disabled={busy}>
        {#if busy}
          正在解锁…
        {:else}
          解锁日记库
        {/if}
      </button>
    </form>

    <footer>
      <small>
        DailyTxT (GPLv3) 风格的 Svelte 前端，本项目同样遵循 GPLv3。
      </small>
    </footer>
  </div>
</div>

<style>
  .unlock-container {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #0f172a, #1e293b);
    color: #e2e8f0;
    padding: 2rem;
  }

  .card {
    width: min(480px, 100%);
    background: rgba(15, 23, 42, 0.85);
    border-radius: 16px;
    padding: 2.5rem 2rem;
    box-shadow: 0 20px 80px rgba(15, 23, 42, 0.65);
    backdrop-filter: blur(12px);
  }

  h1 {
    margin-bottom: 0.25rem;
    font-size: 1.9rem;
    text-align: center;
  }

  .tagline {
    margin-bottom: 2rem;
    text-align: center;
    color: #cbd5f5;
    font-size: 1rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  label {
    font-weight: 600;
    font-size: 0.95rem;
  }

  input[type='password'] {
    padding: 0.75rem 1rem;
    border-radius: 12px;
    border: 1px solid rgba(148, 163, 184, 0.4);
    background: rgba(15, 23, 42, 0.6);
    color: #e2e8f0;
    transition: border 0.2s ease, box-shadow 0.2s ease;
    font-size: 1rem;
  }

  input[type='password']:focus {
    outline: none;
    border-color: #6366f1;
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  select {
    padding: 0.75rem 1rem;
    border-radius: 12px;
    border: 1px solid rgba(148, 163, 184, 0.4);
    background: rgba(15, 23, 42, 0.6);
    color: #e2e8f0;
    transition: border 0.2s ease, box-shadow 0.2s ease;
    font-size: 1rem;
    appearance: none;
  }

  select:focus {
    outline: none;
    border-color: #6366f1;
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  .confirm-toggle {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-size: 0.9rem;
    margin-top: 0.5rem;
    color: #cbd5f5;
    cursor: pointer;
  }

  .confirm-toggle input {
    width: 18px;
    height: 18px;
    cursor: pointer;
  }

  button[type='submit'] {
    margin-top: 1rem;
    padding: 0.85rem 1.25rem;
    border-radius: 12px;
    border: none;
    background: linear-gradient(135deg, #6366f1, #8b5cf6);
    color: white;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  button[type='submit']:disabled {
    opacity: 0.7;
    cursor: wait;
  }

  button[type='submit']:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 15px 40px rgba(99, 102, 241, 0.35);
  }

  button[type='submit']:active:not(:disabled) {
    transform: translateY(0);
  }

  .directory-picker {
    margin-top: 0.75rem;
    padding: 1rem;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.55);
    border: 1px solid rgba(148, 163, 184, 0.25);
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .directory-label {
    margin: 0;
    font-weight: 600;
    font-size: 0.95rem;
  }

  .directory-hint {
    margin: 0;
    font-size: 0.85rem;
    color: #cbd5f5;
    word-break: break-all;
    overflow-wrap: break-word;
  }

  .picker-actions {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .secondary,
  .ghost {
    padding: 0.55rem 0.95rem;
    border-radius: 10px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .secondary {
    border: none;
    background: linear-gradient(135deg, #38bdf8, #6366f1);
    color: #0f172a;
  }

  .secondary:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(56, 189, 248, 0.4);
  }

  .secondary:active {
    transform: translateY(0);
  }

  .ghost {
    background: transparent;
    border: 1px solid rgba(148, 163, 184, 0.4);
    color: #cbd5f5;
  }

  .ghost:hover {
    background: rgba(148, 163, 184, 0.15);
  }

  .error {
    background: rgba(239, 68, 68, 0.18);
    color: #fecdd3;
    padding: 0.6rem 0.8rem;
    border-radius: 10px;
    font-size: 0.9rem;
    border: 1px solid rgba(239, 68, 68, 0.3);
  }

  footer {
    margin-top: 2rem;
    text-align: center;
    color: #94a3b8;
    font-size: 0.875rem;
  }

  /* 平板设备 */
  @media (max-width: 768px) {
    .unlock-container {
      padding: 1.5rem;
    }

    .card {
      padding: 2rem 1.5rem;
    }

    h1 {
      font-size: 1.6rem;
    }

    .tagline {
      font-size: 0.95rem;
      margin-bottom: 1.5rem;
    }
  }

  /* 移动设备 */
  @media (max-width: 480px) {
    .unlock-container {
      padding: 1rem;
    }

    .card {
      padding: 1.5rem 1.25rem;
      border-radius: 12px;
    }

    h1 {
      font-size: 1.4rem;
      margin-bottom: 0.5rem;
    }

    .tagline {
      font-size: 0.9rem;
      margin-bottom: 1.25rem;
    }

    label {
      font-size: 0.9rem;
    }

    input[type='password'] {
      padding: 0.65rem 0.85rem;
      font-size: 0.95rem;
    }

    .confirm-toggle {
      font-size: 0.85rem;
    }

    .confirm-toggle input {
      width: 16px;
      height: 16px;
    }

    button[type='submit'] {
      padding: 0.75rem 1rem;
      font-size: 0.95rem;
    }

    .directory-picker {
      padding: 0.85rem;
    }

    .directory-label {
      font-size: 0.9rem;
    }

    .directory-hint {
      font-size: 0.8rem;
    }

    .picker-actions {
      gap: 0.5rem;
    }

    .secondary,
    .ghost {
      flex: 1;
      text-align: center;
      padding: 0.5rem 0.75rem;
      font-size: 0.9rem;
    }

    .error {
      font-size: 0.85rem;
      padding: 0.55rem 0.7rem;
    }

    footer {
      margin-top: 1.5rem;
      font-size: 0.8rem;
    }
  }

  /* 超小屏设备 */
  @media (max-width: 360px) {
    .unlock-container {
      padding: 0.75rem;
    }

    .card {
      padding: 1.25rem 1rem;
    }

    h1 {
      font-size: 1.25rem;
    }

    .tagline {
      font-size: 0.85rem;
    }

    .secondary,
    .ghost {
      padding: 0.45rem 0.6rem;
      font-size: 0.85rem;
    }
  }

  /* 触摸设备优化 */
  @media (hover: none) and (pointer: coarse) {
    button,
    input,
    .confirm-toggle {
      -webkit-tap-highlight-color: transparent;
    }

    button[type='submit'],
    .secondary,
    .ghost {
      padding: 0.75rem 1rem;
    }
  }

  /* 横屏模式优化 */
  @media (max-width: 768px) and (orientation: landscape) {
    .unlock-container {
      padding: 1rem;
    }

    .card {
      max-width: 600px;
      padding: 1.5rem;
    }

    h1 {
      font-size: 1.5rem;
    }

    .tagline {
      margin-bottom: 1rem;
    }

    form {
      gap: 0.6rem;
    }

    footer {
      margin-top: 1rem;
    }
  }
</style>
