<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { get } from 'svelte/store';
  import {
    activeEntryDetail,
    activeEntryId,
    entries,
    filteredEntries,
    lastSaved,
    searchTerm,
    statusMessage,
    unlocked,
    vaultRoot
  } from '../stores/vault';
  import {
    createVaultEntry,
    decryptImage,
    deleteVaultEntry,
    exportVaultToFile,
    fetchEntries,
  importClipboardImage,
  importVaultImage,
    loadVaultEntry,
    lockVault,
    pickImageFile,
    updateVaultEntry
  } from '../api';
  import type { EntryDetail } from '../types';
  import { marked } from 'marked';

  let localTitle = '';
  let localContent = '';
  let saving = false;
  let saveError: string | null = null;
  let deleting = false;
  let loadingEntries = false;
  let loadingEntry = false;
  let editorTextarea: HTMLTextAreaElement | null = null;

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let loadedEntryId: string | null = null;

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
      const currentEntries = get(entries);
      if (currentEntries.length > 0) {
        loadingEntries = false;
        return;
      }
      const items = await fetchEntries();
      entries.set(items);
      const existing = get(activeEntryId);
      if (!existing && items.length > 0) {
        activeEntryId.set(items[0].id);
      }
      if (items.length === 0) {
        activeEntryDetail.set(null);
        loadedEntryId = null;
        localTitle = '';
        localContent = '';
      }
    } catch (err) {
      saveError = err instanceof Error ? err.message : '无法读取日记条目';
    } finally {
      loadingEntries = false;
    }
  }

  async function loadEntryDetail(id: string) {
    loadingEntry = true;
    try {
      const entry = await loadVaultEntry(id);
      entries.update((items) =>
        items.map((item) =>
          item.id === entry.id
            ? { ...item, title: entry.title, updated_at: entry.updated_at }
            : item
        )
      );
      activeEntryDetail.set(entry);
      loadedEntryId = entry.id;
      localTitle = entry.title;
      localContent = entry.content;
      saveError = null;
    } catch (err) {
      const message = err instanceof Error ? err.message : '无法读取日记条目';
      saveError = message;
    } finally {
      loadingEntry = false;
    }
  }

  function selectEntry(id: string) {
    if (id === get(activeEntryId)) return;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    activeEntryId.set(id);
    activeEntryDetail.set(null);
    loadedEntryId = null;
    localTitle = '';
    localContent = '';
    saveError = null;
    statusMessage.set('');
    void loadEntryDetail(id);
  }

  function scheduleSave() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(saveActiveEntry, 600);
  }

  async function saveActiveEntry() {
    const detail = get(activeEntryDetail);
    if (!detail) return;
    debounceTimer = null;
    saving = true;
    saveError = null;
    statusMessage.set('');
    try {
      const updated: EntryDetail = await updateVaultEntry({
        ...detail,
        title: localTitle,
        content: localContent
      });
      activeEntryDetail.set(updated);
      entries.update((items) =>
        items.map((item) =>
          item.id === updated.id
            ? { ...item, title: updated.title, updated_at: updated.updated_at }
            : item
        )
      );
      loadedEntryId = updated.id;
      localTitle = updated.title;
      localContent = updated.content;
      lastSaved.set(updated.updated_at ?? null);
      statusMessage.set('已保存');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '保存失败';
    } finally {
      saving = false;
    }
  }

  async function handleCreate() {
    try {
      const entry = await createVaultEntry('新的日记', '');
      const { content, ...summary } = entry;
      entries.update((items) => [summary, ...items]);
      activeEntryDetail.set(entry);
      activeEntryId.set(entry.id);
      loadedEntryId = entry.id;
      localTitle = entry.title;
      localContent = entry.content;
      statusMessage.set('已创建新的日记');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '无法创建日记';
    }
  }

  async function handleDelete() {
    const detail = get(activeEntryDetail);
    if (!detail) return;
    if (!confirm('确定要删除当前日记吗？操作不可撤销。')) {
      return;
    }
    deleting = true;
    try {
      await deleteVaultEntry(detail.id);
      entries.update((items) => items.filter((item) => item.id !== detail.id));
      activeEntryDetail.set(null);
      loadedEntryId = null;
      const remaining = get(entries);
      const next = remaining[0];
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

  async function handleInsertImage() {
    const detail = get(activeEntryDetail);
    if (!detail) return;
    try {
      const file = await pickImageFile();
      if (!file) return;
      const storedPath = await importVaultImage(file);
      const normalized = storedPath.replace(/\\/g, '/');
      insertAtCursor(`\n\n![插图](${normalized})\n\n`);
      saveError = null;
      statusMessage.set('已插入图片');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '插入图片失败';
    }
  }

  async function handlePaste(event: ClipboardEvent) {
    const detail = get(activeEntryDetail);
    if (!detail) return;
    const clipboard = event.clipboardData;
    if (!clipboard) return;

    const items = Array.from(clipboard.items ?? []);
    const imageItem = items.find((item) => item.kind === 'file' && item.type.startsWith('image/'));
    if (!imageItem) {
      return;
    }

    const file = imageItem.getAsFile();
    if (!file) {
      return;
    }

    event.preventDefault();
    try {
      const buffer = new Uint8Array(await file.arrayBuffer());
      const storedPath = await importClipboardImage({
        name: file.name || null,
        mime: file.type || null,
        data: buffer
      });
      const normalized = storedPath.replace(/\\/g, '/');
      const altText = file.name ? file.name.replace(/\.[^.]+$/, '') : '粘贴的图片';
      insertAtCursor(`\n\n![${altText}](${normalized})\n\n`);
      saveError = null;
      statusMessage.set('已粘贴图片');
    } catch (err) {
      saveError = err instanceof Error ? err.message : '粘贴图片失败';
    }
  }

  function insertAtCursor(snippet: string) {
    if (!editorTextarea) {
      localContent = `${localContent}${snippet}`;
      scheduleSave();
      return;
    }
    const start = editorTextarea.selectionStart ?? localContent.length;
    const end = editorTextarea.selectionEnd ?? start;
    localContent = `${localContent.slice(0, start)}${snippet}${localContent.slice(end)}`;
    const cursor = start + snippet.length;
    setTimeout(() => {
      if (editorTextarea) {
        editorTextarea.selectionStart = cursor;
        editorTextarea.selectionEnd = cursor;
        editorTextarea.focus();
      }
    });
    scheduleSave();
  }

  async function handleLock() {
    await lockVault();
    unlocked.set(false);
    entries.set([]);
    activeEntryId.set(null);
    activeEntryDetail.set(null);
    localTitle = '';
    localContent = '';
    loadedEntryId = null;
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

  let currentDetail: EntryDetail | null = null;

  $: currentDetail = $activeEntryDetail;

  $: if (!$unlocked) {
    loadedEntryId = null;
    localTitle = '';
    localContent = '';
  }

  $: if ($unlocked && $activeEntryId && loadedEntryId !== $activeEntryId && !loadingEntry) {
    void loadEntryDetail($activeEntryId);
  }

  // 图片缓存：路径 -> data URL
  let imageCache = new Map<string, string>();

  async function resolveImageSource(root: string | null, href: string): Promise<string> {
    if (!href) return '';
    if (/^(https?:|data:|file:|tauri)/i.test(href)) {
      return href;
    }
    if (!root) return href;
    
    const base = root.replace(/[\\/]+$/, '');
    const relative = href.replace(/^[/\\]+/, '').replace(/\\/g, '/');
    const fullPath = `${base}/${relative}`;
    
    // 检查缓存
    if (imageCache.has(fullPath)) {
      return imageCache.get(fullPath)!;
    }
    
    try {
      // 解密图片
      const data = await decryptImage(fullPath);
      // 转换为 data URL
      const blob = new Blob([data as any]);
      const dataUrl = await new Promise<string>((resolve) => {
        const reader = new FileReader();
        reader.onloadend = () => resolve(reader.result as string);
        reader.readAsDataURL(blob);
      });
      
      imageCache.set(fullPath, dataUrl);
      return dataUrl;
    } catch (err) {
      console.error('Failed to decrypt image:', err);
      return '';
    }
  }

  let previewHtml = '';
  let isRenderingPreview = false;

  // 异步渲染预览 HTML
  async function renderPreview(content: string, root: string | null) {
    if (isRenderingPreview) return;
    isRenderingPreview = true;
    
    try {
      const renderer = new marked.Renderer();
      const imagePromises: Promise<void>[] = [];
      const imageMap = new Map<string, string>();
      
      // 第一遍：收集所有图片
      renderer.image = ({ href = '', title, text }) => {
        const placeholder = `__IMAGE_PLACEHOLDER_${imageMap.size}__`;
        imageMap.set(placeholder, href);
        imagePromises.push(
          resolveImageSource(root, href).then(src => {
            imageMap.set(placeholder, src);
          })
        );
        return placeholder;
      };
      
      let html = await marked.parse(content || '', { renderer });
      
      // 等待所有图片解密
      await Promise.all(imagePromises);
      
      // 第二遍：替换占位符为实际图片
      for (const [placeholder, src] of imageMap.entries()) {
        if (src.startsWith('data:') || src.startsWith('http')) {
          const alt = placeholder;
          html = html.replace(placeholder, `<img src="${src}" alt="${alt}">`);
        }
      }
      
      previewHtml = html;
    } finally {
      isRenderingPreview = false;
    }
  }

  $: if (localContent || $vaultRoot) {
    void renderPreview(localContent, $vaultRoot);
  }
</script>

<div class="layout">
  <aside>
    <div class="header">
      <h2>日记列表</h2>
      <button class="primary" on:click={handleCreate}>新建</button>
    </div>
    <div class="location">
      <span class="location-label">📁 存储目录</span>
      <span class="location-path" title={$vaultRoot ?? '应用数据目录（默认）'}>
        {$vaultRoot ?? '应用数据目录（默认）'}
      </span>
    </div>
    <input
      class="search"
      type="search"
      placeholder="搜索标题"
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
    {#if loadingEntry && !currentDetail}
      <div class="empty">
        <h2>正在加载</h2>
        <p>正在读取选中的日记内容…</p>
      </div>
    {:else if currentDetail}
      <div class="editor-header">
        <div>
          <h2>编辑日记</h2>
          <p class="meta">
            创建：{formatDate(currentDetail.created_at)} · 修改：{formatDate(currentDetail.updated_at)}
          </p>
        </div>
        <div class="tools">
          <button class="ghost" on:click={handleExport}>导出</button>
          <button class="ghost" on:click={handleInsertImage}>插入图片</button>
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
        bind:this={editorTextarea}
        bind:value={localContent}
        on:input={scheduleSave}
        on:paste={handlePaste}
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
          {:else if $statusMessage}
            {$statusMessage}
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

  .location {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.6rem 0.75rem;
    border-radius: 10px;
    background: rgba(15, 23, 42, 0.55);
    border: 1px solid rgba(148, 163, 184, 0.3);
  }

  .location-label {
    font-size: 0.82rem;
    font-weight: 600;
    color: #cbd5f5;
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .location-path {
    font-size: 0.78rem;
    color: #94a3b8;
    word-break: break-all;
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

  .ghost:disabled,
  .danger:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .ghost:hover:not(:disabled) {
    background: rgba(148, 163, 184, 0.15);
  }

  .danger {
    border-color: rgba(239, 68, 68, 0.55);
    color: #fca5a5;
  }

  .danger:hover:not(:disabled) {
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

  .preview-content :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 8px;
    margin: 0.5rem 0;
    display: block;
  }

  .preview-content.empty-preview {
    color: #94a3b8;
  }

  .footer-row {
    display: flex;
    justify-content: space-between;
    color: #94a3b8;
    font-size: 0.9rem;
    margin-top: 0.5rem;
  }

  .status {
    display: flex;
    gap: 0.35rem;
    align-items: center;
  }

  .count {
    font-variant-numeric: tabular-nums;
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
