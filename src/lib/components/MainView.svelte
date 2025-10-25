<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    activeEntry,
    activeEntryId,
    entries,
    filteredEntries,
    lastSaved,
    searchTerm,
    statusMessage,
    unlocked
  } from '../stores/vault';
  import {
    createVaultEntry,
    deleteVaultEntry,
    exportVaultToFile,
    fetchEntries,
    lockVault,
    updateVaultEntry
  } from '../api';
  import type { Entry } from '../types';
  import { marked } from 'marked';

  let localTitle = '';
  let localContent = '';
  let saving = false;
  let saveError: string | null = null;
  let deleting = false;
  let loadingEntries = false;

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  const formatDate = (value: string | null | undefined) => {
    if (!value) return '—';
    try {
      return new Intl.DateTimeFormat(undefined, {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit'
      }).format(new Date(value));
    } catch (err) {
      return value;
    }
  };

  async function ensureEntriesLoaded() {
    loadingEntries = true;
    try {
      if ($entries.length > 0) {
        loadingEntries = false;
        return;
      }
      const items = await fetchEntries();
      entries.set(items);
      if (items.length > 0) {
        const latest = items[0];
        activeEntryId.set(latest.id);
      }
    } catch (err) {
      saveError = err instanceof Error ? err.message : '无法读取日记条目';
    } finally {
      loadingEntries = false;
    }
  }

  $: currentEntry = $activeEntry;
  $: if (currentEntry) {
    localTitle = currentEntry.title;
    localContent = currentEntry.content;
  }

  $: previewHtml = marked.parse(localContent || '');

  async function handleCreate() {
    try {
      const entry = await createVaultEntry('新的日记', '');
      entries.update((items) => [entry, ...items]);
      activeEntryId.set(entry.id);
      localTitle = entry.title;
      localContent = entry.content;
      statusMessage.set('已创建新的日记');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '无法创建日记';
    }
  }

  function selectEntry(id: string) {
    if (id === $activeEntryId) return;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    activeEntryId.set(id);
    saveError = null;
  }

  function scheduleSave() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(saveActiveEntry, 600);
  }

  async function saveActiveEntry() {
    if (!currentEntry) return;
    debounceTimer = null;
    saving = true;
    saveError = null;
    try {
      const updated: Entry = await updateVaultEntry({
        ...currentEntry,
        title: localTitle,
        content: localContent
      });
      entries.update((items) =>
        items.map((item) => (item.id === updated.id ? updated : item))
      );
      activeEntryId.set(updated.id);
      lastSaved.set(updated.updated_at ?? null);
      statusMessage.set('已保存');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '保存失败';
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (!currentEntry) return;
    if (!confirm('确定要删除当前日记吗？操作不可撤销。')) {
      return;
    }
    deleting = true;
    try {
      await deleteVaultEntry(currentEntry.id);
      entries.update((items) => items.filter((item) => item.id !== currentEntry.id));
      const next = $entries[0];
      activeEntryId.set(next ? next.id : null);
      if (!next) {
        localTitle = '';
        localContent = '';
      }
      statusMessage.set('日记已删除');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '无法删除日记';
    } finally {
      deleting = false;
    }
  }

  async function handleExport() {
    try {
      const path = await exportVaultToFile();
      if (path) {
        statusMessage.set(`已导出到 ${path}`);
      }
    } catch (err) {
      saveError = err instanceof Error ? err.message : '导出失败';
    }
  }

  async function handleLock() {
    await lockVault();
    unlocked.set(false);
    entries.set([]);
    activeEntryId.set(null);
    localTitle = '';
    localContent = '';
    statusMessage.set('已锁定');
  }

  onDestroy(() => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
  });

  onMount(() => {
    ensureEntriesLoaded();
  });
</script>

<div class="layout">
  <aside>
    <div class="header">
      <h2>日记列表</h2>
      <button class="primary" on:click={handleCreate}>新建</button>
    </div>
    <input
      class="search"
      type="search"
      placeholder="搜索标题或内容"
      bind:value={$searchTerm}
    />
    {#if loadingEntries}
      <div class="hint">正在加载…</div>
    {:else if $filteredEntries.length === 0}
      <div class="hint">暂无日记条目</div>
    {:else}
      <ul class="entry-list">
        {#each $filteredEntries as item (item.id)}
          <li class:item-active={item.id === $activeEntryId}>
            <button type="button" on:click={() => selectEntry(item.id)}>
              <h3>{item.title || '未命名日记'}</h3>
              <p>{formatDate(item.updated_at)}</p>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <section class="editor">
    {#if currentEntry}
      <div class="editor-header">
        <div>
          <h2>编辑日记</h2>
          <p class="meta">
            创建：{formatDate(currentEntry.created_at)} · 修改：{formatDate(currentEntry.updated_at)}
          </p>
        </div>
        <div class="tools">
          <button class="ghost" on:click={handleExport}>导出</button>
          <button class="ghost" on:click={handleLock}>锁定</button>
          <button class="danger" on:click={handleDelete} disabled={deleting}>删除</button>
        </div>
      </div>

      <label class="input-label" for="entry-title">标题</label>
      <input
        class="title-input"
        id="entry-title"
        type="text"
        bind:value={localTitle}
        on:input={scheduleSave}
        placeholder="为日记命名"
      />

      <label class="input-label" for="entry-content">内容</label>
      <textarea
        id="entry-content"
        bind:value={localContent}
        on:input={scheduleSave}
        placeholder="开始记录你的每一天…"
      ></textarea>

      <div class="preview-card">
        <div class="preview-header">
          <h3>预览</h3>
          <span class="hint-text">Markdown 渲染仅在本地执行</span>
        </div>
        <div class="preview-content" class:empty-preview={!localContent}>
          {#if localContent.trim().length === 0}
            <em>暂无内容，开始书写后这里会实时预览 Markdown。</em>
          {:else}
            <article class="markdown">{@html previewHtml}</article>
          {/if}
        </div>
      </div>

      <div class="footer-row">
        <span class="status">
          {#if saving}
            正在保存…
          {:else if saveError}
            ⚠️ {saveError}
          {:else if $lastSaved}
            上次保存：{formatDate($lastSaved)}
          {:else}
            已加载
          {/if}
        </span>
        <span class="count">{localContent.length} 字符</span>
      </div>
    {:else}
      <div class="empty">
        <h2>欢迎！</h2>
        <p>创建第一篇日记或从左侧选择已有条目。</p>
        <button class="primary" on:click={handleCreate}>立即开始写作</button>
      </div>
    {/if}
  </section>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 320px 1fr;
    height: 100vh;
    background: #0f172a;
    color: #e2e8f0;
  }

  aside {
    border-right: 1px solid rgba(148, 163, 184, 0.2);
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    background: rgba(15, 23, 42, 0.96);
    backdrop-filter: blur(6px);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .primary {
    background: linear-gradient(135deg, #6366f1, #8b5cf6);
    border: none;
    color: white;
    padding: 0.5rem 0.9rem;
    border-radius: 10px;
    cursor: pointer;
    font-weight: 600;
    transition: transform 0.15s ease;
  }

  .primary:hover {
    transform: translateY(-1px);
  }

  .search {
    padding: 0.65rem 0.9rem;
    border-radius: 10px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: rgba(15, 23, 42, 0.8);
    color: inherit;
  }

  .entry-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow-y: auto;
  }

  .entry-list li {
    list-style: none;
  }

  .entry-list li button {
    width: 100%;
    text-align: left;
    padding: 0.85rem 0.9rem;
    border-radius: 12px;
    cursor: pointer;
    background: rgba(15, 23, 42, 0.6);
    border: 1px solid transparent;
    color: inherit;
    transition: background 0.15s ease, border 0.15s ease;
  }

  .entry-list li button:hover,
  .entry-list li button:focus-visible {
    border-color: rgba(99, 102, 241, 0.5);
    outline: none;
  }

  .entry-list li.item-active button {
    background: rgba(99, 102, 241, 0.15);
    border-color: rgba(99, 102, 241, 0.7);
  }

  .entry-list h3 {
    margin: 0;
    font-size: 1rem;
  }

  .entry-list p {
    margin: 0.35rem 0 0;
    font-size: 0.82rem;
    color: #94a3b8;
  }

  .hint {
    padding: 1rem;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.5);
    text-align: center;
    color: #94a3b8;
  }

  .editor {
    padding: 2rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    background: radial-gradient(circle at top left, rgba(99, 102, 241, 0.18), transparent 55%),
      #0f172a;
  }

  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .meta {
    color: #94a3b8;
    margin: 0.25rem 0 0;
  }

  .tools {
    display: flex;
    gap: 0.5rem;
  }

  .ghost,
  .danger {
    padding: 0.5rem 0.9rem;
    border-radius: 10px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .ghost:hover {
    background: rgba(148, 163, 184, 0.15);
  }

  .danger {
    border-color: rgba(239, 68, 68, 0.55);
    color: #fca5a5;
  }

  .danger:hover {
    background: rgba(239, 68, 68, 0.12);
  }

  .input-label {
    font-weight: 600;
    margin-top: 0.5rem;
  }

  .title-input,
  textarea {
    width: 100%;
    border-radius: 12px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: rgba(15, 23, 42, 0.6);
    color: inherit;
    padding: 0.75rem 1rem;
    font-size: 1rem;
    transition: border 0.2s ease, box-shadow 0.2s ease;
  }

  .title-input:focus,
  textarea:focus {
    outline: none;
    border-color: rgba(99, 102, 241, 0.8);
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  textarea {
    min-height: 320px;
    resize: vertical;
    line-height: 1.6;
  }

  .preview-card {
    margin-top: 1.5rem;
    padding: 1.25rem;
    border-radius: 16px;
    border: 1px solid rgba(148, 163, 184, 0.25);
    background: rgba(15, 23, 42, 0.55);
    backdrop-filter: blur(6px);
  }

  .preview-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 0.85rem;
  }

  .preview-header h3 {
    margin: 0;
    font-size: 1.1rem;
  }

  .hint-text {
    color: #94a3b8;
    font-size: 0.8rem;
  }

  .preview-content {
    min-height: 180px;
    max-height: 400px;
    overflow-y: auto;
    padding-right: 0.25rem;
  }

  .preview-content.empty-preview {
    color: #94a3b8;
  }

  :global(.markdown) h1,
  :global(.markdown) h2,
  :global(.markdown) h3,
  :global(.markdown) h4,
  :global(.markdown) h5,
  :global(.markdown) h6 {
    margin: 1.2rem 0 0.6rem;
    font-weight: 600;
  }

  :global(.markdown) p {
    margin: 0.75rem 0;
    line-height: 1.7;
  }

  :global(.markdown) ul,
  :global(.markdown) ol {
    padding-left: 1.4rem;
    margin: 0.75rem 0;
  }

  :global(.markdown) code {
    background: rgba(15, 23, 42, 0.8);
    padding: 0.1rem 0.35rem;
    border-radius: 6px;
    font-family: 'Fira Code', Consolas, monospace;
    font-size: 0.92rem;
  }

  :global(.markdown) pre {
    background: rgba(15, 23, 42, 0.8);
    padding: 0.75rem 1rem;
    border-radius: 12px;
    overflow-x: auto;
    margin: 1rem 0;
  }

  :global(.markdown) blockquote {
    margin: 0.9rem 0;
    padding-left: 1rem;
    border-left: 3px solid rgba(99, 102, 241, 0.5);
    color: #cbd5f5;
  }

  .footer-row {
    display: flex;
    justify-content: space-between;
    color: #94a3b8;
    font-size: 0.9rem;
    margin-top: 0.5rem;
  }

  .empty {
    margin: auto;
    max-width: 420px;
    text-align: center;
    background: rgba(15, 23, 42, 0.6);
    padding: 2.5rem 2rem;
    border-radius: 18px;
    border: 1px solid rgba(99, 102, 241, 0.3);
  }

  .empty h2 {
    margin-bottom: 0.5rem;
  }

  @media (max-width: 960px) {
    .layout {
      grid-template-columns: 1fr;
      grid-template-rows: 260px 1fr;
      height: auto;
      min-height: 100vh;
    }

    aside {
      border-right: none;
      border-bottom: 1px solid rgba(148, 163, 184, 0.2);
    }
  }
</style>
