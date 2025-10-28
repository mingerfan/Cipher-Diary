<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
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

  let localTitle = $state('');
  let localContent = $state('');
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let deleting = $state(false);
  let loadingEntries = $state(false);
  let loadingEntry = $state(false);
  let editorTextarea = $state<HTMLTextAreaElement | null>(null);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let loadedEntryId: string | null = null;

  // 视图切换状态: 'list' | 'editor'
  let currentView = $state<'list' | 'editor'>('list');

  // 检测屏幕尺寸
  let isLargeScreen = $state(true);

  // 大屏幕侧边栏折叠状态（从localStorage读取）
  let sidebarCollapsed = $state(false);

  // 全局状态镜像，便于运行时依赖跟踪
  let isUnlocked = $state(get(unlocked));
  let activeEntryIdValue = $state<string | null>(get(activeEntryId));
  let vaultRootValue = $state<string | null>(get(vaultRoot));
  let currentDetail = $state<EntryDetail | null>(get(activeEntryDetail));

  function updateScreenSize() {
    if (typeof window === 'undefined') return;
    isLargeScreen = window.innerWidth > 768;
  }

  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
    if (typeof window !== 'undefined') {
      localStorage.setItem('diary-sidebar-collapsed', String(sidebarCollapsed));
    }
  }

  onMount(() => {
    updateScreenSize();

    const unsubscribes: Array<() => void> = [];

    const key_handler = (e: KeyboardEvent) => {
      if (e.key === 'l' && e.ctrlKey) {
        handleLock();
      }
    }
    window.addEventListener('keydown', key_handler);

    if (typeof window !== 'undefined') {
      window.addEventListener('resize', updateScreenSize);

      const saved = localStorage.getItem('diary-sidebar-collapsed');
      if (saved !== null) {
        sidebarCollapsed = saved === 'true';
      }
    }

    unsubscribes.push(
      unlocked.subscribe((value) => {
        isUnlocked = value;
      })
    );
    unsubscribes.push(
      activeEntryId.subscribe((value) => {
        activeEntryIdValue = value;
      })
    );
    unsubscribes.push(
      vaultRoot.subscribe((value) => {
        vaultRootValue = value;
      })
    );
    unsubscribes.push(
      activeEntryDetail.subscribe((value) => {
        currentDetail = value;
      })
    );

    void ensureEntriesLoaded();

    return () => {
      if (typeof window !== 'undefined') {
        window.removeEventListener('resize', updateScreenSize);
        window.removeEventListener('keydown', key_handler);
      }
      unsubscribes.forEach((fn) => fn());
    };
  });

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
  const existing = activeEntryIdValue;
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
    if (id === activeEntryIdValue) {
      if (!isLargeScreen) {
        currentView = 'editor';
      }
      return;
    }
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
    if (!isLargeScreen) {
      currentView = 'editor';
    }
  }

  function scheduleSave() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(saveActiveEntry, 600);
  }

  async function saveActiveEntry() {
    const detail = currentDetail;
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
    const detail = currentDetail;
    if (!detail) return;
    if (!await confirm('确定要删除当前日记吗？操作不可撤销。')) {
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
    const detail = currentDetail;
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
    const detail = currentDetail;
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
      debounceTimer = null;
    }
  });

  const imageCache = new Map<string, string>();

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

  let previewHtml = $state('');
  let previewToken = 0;

  async function renderPreview(content: string, root: string | null) {
    const token = ++previewToken;
    if (!content.trim()) {
      if (token === previewToken) {
        previewHtml = '';
      }
      return;
    }

    const renderer = new marked.Renderer();
    const imagePromises: Promise<void>[] = [];
    const imageMap = new Map<string, { alt: string; src: string }>();

    renderer.image = ({ href = '', text = '' }) => {
      const placeholder = `__IMAGE_PLACEHOLDER_${imageMap.size}__`;
      const alt = text.trim() || '插图';
      imageMap.set(placeholder, { alt, src: href });
      imagePromises.push(
        resolveImageSource(root, href).then((resolved) => {
          imageMap.set(placeholder, { alt, src: resolved });
        })
      );
      return placeholder;
    };

    let html = await marked.parse(content || '', { renderer });

    await Promise.all(imagePromises);

    for (const [placeholder, data] of imageMap.entries()) {
      const { alt, src } = data;
      if (!src) {
        html = html.replace(placeholder, '');
        continue;
      }
      const safeAlt = escapeAttribute(alt);
      html = html.replace(placeholder, `<img src="${src}" alt="${safeAlt}">`);
    }

    if (token === previewToken) {
      previewHtml = html;
    }
  }

  function escapeAttribute(value: string): string {
    return value.replace(/['"&<>]/g, (char) => attributeEscapeMap[char] ?? char);
  }

  const attributeEscapeMap: Record<string, string> = {
    '"': '&quot;',
    "'": '&#39;',
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;'
  };

  $effect(() => {
    if (!isUnlocked) {
      loadedEntryId = null;
      localTitle = '';
      localContent = '';
    }
  });

  $effect(() => {
    if (!isUnlocked) {
      return;
    }

    const id = activeEntryIdValue;
    if (!id || loadingEntry) {
      return;
    }

    if (loadedEntryId === id) {
      return;
    }

    void loadEntryDetail(id);
  });

  $effect(() => {
    const content = localContent;
    const root = vaultRootValue;
    if (!content && !root) {
      previewHtml = '';
      return;
    }
    void renderPreview(content, root);
  });
</script>

<div class="app-container" class:large-screen={isLargeScreen}>
  <div class="layout" class:sidebar-collapsed={isLargeScreen && sidebarCollapsed}>
    <!-- 日记列表视图 -->
    <aside class:hidden={(!isLargeScreen && currentView !== 'list') || (isLargeScreen && sidebarCollapsed)}>
    <div class="header">
      <h2>日记列表</h2>
      <button class="primary" onclick={handleCreate}>新建</button>
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
            <button type="button" onclick={() => selectEntry(item.id)}>
              <h3>{item.title || '未命名日记'}</h3>
              <p>{formatDate(item.updated_at)}</p>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <!-- 编辑器视图 -->
  <section class="editor" class:hidden={!isLargeScreen && currentView !== 'editor'}>
    {#if loadingEntry && !currentDetail}
      <div class="empty">
        <h2>正在加载</h2>
        <p>正在读取选中的日记内容…</p>
      </div>
    {:else if currentDetail}
      <div class="editor-header">
        <!-- 大屏幕侧边栏切换按钮（集成到header） -->
        {#if isLargeScreen}
          <button class="sidebar-toggle" onclick={toggleSidebar} title={sidebarCollapsed ? '展开侧边栏' : '折叠侧边栏'}>
            {#if sidebarCollapsed}
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
                <line x1="9" y1="3" x2="9" y2="21"></line>
              </svg>
            {:else}
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="11 17 6 12 11 7"></polyline>
                <polyline points="18 17 13 12 18 7"></polyline>
              </svg>
            {/if}
          </button>
        {/if}
        
        <div class="header-content">
          <h2>编辑日记</h2>
          <p class="meta">
            创建：{formatDate(currentDetail.created_at)} · 修改：{formatDate(currentDetail.updated_at)}
          </p>
        </div>
        <div class="tools">
          <button class="ghost" onclick={handleExport}>导出</button>
          <button class="ghost" onclick={handleInsertImage}>插入图片</button>
          <button class="ghost" onclick={handleLock}>锁定</button>
          <button class="danger" onclick={handleDelete} disabled={deleting}>删除</button>
        </div>
      </div>

      <label class="input-label" for="entry-title">标题</label>
      <input
        class="title-input"
        id="entry-title"
        type="text"
        bind:value={localTitle}
  oninput={scheduleSave}
        placeholder="为日记命名"
      />

      <label class="input-label" for="entry-content">内容</label>
      <textarea
        id="entry-content"
        bind:this={editorTextarea}
        bind:value={localContent}
  oninput={scheduleSave}
  onpaste={handlePaste}
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
  <button class="primary" onclick={handleCreate}>立即开始写作</button>
      </div>
    {/if}
  </section>
  </div>

  <!-- 小屏：底部导航 -->
  {#if !isLargeScreen}
    <nav class="bottom-nav">
      <button 
        class="nav-item" 
  class:active={currentView === 'list'}
  onclick={() => currentView = 'list'}
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="8" y1="6" x2="21" y2="6"></line>
          <line x1="8" y1="12" x2="21" y2="12"></line>
          <line x1="8" y1="18" x2="21" y2="18"></line>
          <line x1="3" y1="6" x2="3.01" y2="6"></line>
          <line x1="3" y1="12" x2="3.01" y2="12"></line>
          <line x1="3" y1="18" x2="3.01" y2="18"></line>
        </svg>
        <span>列表</span>
      </button>
      <button 
        class="nav-item" 
  class:active={currentView === 'editor'}
  onclick={() => currentView = 'editor'}
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path>
          <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path>
        </svg>
        <span>编辑</span>
      </button>
    </nav>
  {/if}
</div>

<style>
  .app-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #0f172a;
    color: #e2e8f0;
  }

  /* 底部导航栏（小屏） */
  .bottom-nav {
    height: 64px;
    background: rgba(15, 23, 42, 0.98);
    border-top: 1px solid rgba(148, 163, 184, 0.2);
    display: flex;
    justify-content: space-around;
    align-items: center;
    padding: 0 1rem;
    backdrop-filter: blur(10px);
    flex-shrink: 0;
  }

  .bottom-nav .nav-item {
    flex: 1;
    max-width: 120px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.3rem;
    padding: 0.6rem;
    border-radius: 10px;
    background: transparent;
    border: none;
    color: #94a3b8;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 0.75rem;
    font-weight: 500;
  }

  .bottom-nav .nav-item:hover {
    background: rgba(99, 102, 241, 0.1);
    color: #c7d2fe;
  }

  .bottom-nav .nav-item.active {
    background: rgba(99, 102, 241, 0.15);
    color: #a5b4fc;
  }

  .bottom-nav .nav-item svg {
    width: 24px;
    height: 24px;
  }

  .layout {
    flex: 1;
    display: grid;
    overflow: hidden;
    transition: grid-template-columns 0.3s ease;
  }

  .app-container.large-screen .layout {
    grid-template-columns: 320px 1fr;
    height: 100vh;
  }

  .app-container.large-screen .layout.sidebar-collapsed {
    grid-template-columns: 1fr;
  }

  .app-container:not(.large-screen) .layout {
    grid-template-columns: 1fr;
    height: calc(100vh - 64px);
  }

  .hidden {
    display: none !important;
  }

  aside {
    border-right: 1px solid rgba(148, 163, 184, 0.2);
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    background: rgba(15, 23, 42, 0.96);
    backdrop-filter: blur(6px);
    overflow-y: auto;
    height: 100%;
  }

  .app-container:not(.large-screen) aside {
    border-right: none;
    height: 100%;
  }

  /* 侧边栏滚动条样式 */
  aside::-webkit-scrollbar {
    width: 6px;
  }

  aside::-webkit-scrollbar-track {
    background: rgba(15, 23, 42, 0.3);
    border-radius: 10px;
    margin: 0.5rem 0;
  }

  aside::-webkit-scrollbar-thumb {
    background: rgba(99, 102, 241, 0.3);
    border-radius: 10px;
  }

  aside::-webkit-scrollbar-thumb:hover {
    background: rgba(99, 102, 241, 0.5);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.75rem;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    transition: transform 0.15s ease, box-shadow 0.15s ease;
    white-space: nowrap;
  }

  .primary:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(99, 102, 241, 0.4);
  }

  .primary:active {
    transform: translateY(0);
  }

  .search {
    padding: 0.65rem 0.9rem;
    border-radius: 10px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: rgba(15, 23, 42, 0.8);
    color: inherit;
    width: 100%;
    font-size: 0.95rem;
  }

  .search:focus {
    outline: none;
    border-color: rgba(99, 102, 241, 0.8);
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  .entry-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow-y: auto;
    flex: 1;
    padding-right: 0.25rem;
  }

  /* 日记列表滚动条样式 */
  .entry-list::-webkit-scrollbar {
    width: 6px;
  }

  .entry-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .entry-list::-webkit-scrollbar-thumb {
    background: rgba(99, 102, 241, 0.25);
    border-radius: 10px;
  }

  .entry-list::-webkit-scrollbar-thumb:hover {
    background: rgba(99, 102, 241, 0.4);
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
    transition: all 0.15s ease;
  }

  .entry-list li button:hover,
  .entry-list li button:focus-visible {
    border-color: rgba(99, 102, 241, 0.5);
    outline: none;
    transform: translateX(2px);
  }

  .entry-list li.item-active button {
    background: rgba(99, 102, 241, 0.15);
    border-color: rgba(99, 102, 241, 0.7);
  }

  .entry-list h3 {
    margin: 0;
    font-size: 1rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    overflow-y: auto;
    height: 100%;
  }

  /* 编辑器区域滚动条样式 */
  .editor::-webkit-scrollbar {
    width: 8px;
  }

  .editor::-webkit-scrollbar-track {
    background: rgba(15, 23, 42, 0.3);
    border-radius: 10px;
  }

  .editor::-webkit-scrollbar-thumb {
    background: rgba(99, 102, 241, 0.35);
    border-radius: 10px;
  }

  .editor::-webkit-scrollbar-thumb:hover {
    background: rgba(99, 102, 241, 0.5);
  }

  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: wrap;
  }

  /* 侧边栏切换按钮（集成到header） */
  .sidebar-toggle {
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    padding: 0;
    border-radius: 10px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: rgba(15, 23, 42, 0.6);
    color: #94a3b8;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s ease;
  }

  .sidebar-toggle:hover {
    background: rgba(99, 102, 241, 0.2);
    border-color: rgba(99, 102, 241, 0.5);
    color: #c7d2fe;
    transform: scale(1.05);
  }

  .sidebar-toggle:active {
    transform: scale(0.98);
  }

  .sidebar-toggle svg {
    width: 20px;
    height: 20px;
  }

  .header-content {
    flex: 1;
    min-width: 200px;
  }

  .header-content h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .meta {
    color: #94a3b8;
    margin: 0.25rem 0 0;
    font-size: 0.9rem;
  }

  .tools {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .ghost,
  .danger {
    padding: 0.5rem 0.9rem;
    border-radius: 10px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    font-size: 0.9rem;
  }

  .ghost:disabled,
  .danger:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .ghost:hover:not(:disabled) {
    background: rgba(148, 163, 184, 0.15);
    transform: translateY(-1px);
  }

  .ghost:active:not(:disabled) {
    transform: translateY(0);
  }

  .danger {
    border-color: rgba(239, 68, 68, 0.55);
    color: #fca5a5;
  }

  .danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    border-color: rgba(239, 68, 68, 0.8);
    transform: translateY(-1px);
  }

  .danger:active:not(:disabled) {
    transform: translateY(0);
  }

  .input-label {
    font-weight: 600;
    margin-top: 0.5rem;
    font-size: 0.95rem;
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
    font-family: inherit;
    box-sizing: border-box;
  }

  .title-input:focus,
  textarea:focus {
    outline: none;
    border-color: rgba(99, 102, 241, 0.8);
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.25);
  }

  textarea {
    min-height: 280px;
    resize: vertical;
    line-height: 1.6;
  }

  .preview-card {
    margin-top: 1.5rem;
    padding: 0;
    border-radius: 16px;
    border: 1px solid rgba(148, 163, 184, 0.25);
    background: rgba(15, 23, 42, 0.55);
    backdrop-filter: blur(6px);
    overflow: hidden;
    box-sizing: border-box;
    flex-shrink: 0;
  }

  .preview-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 0;
    padding: 1.25rem 1.25rem 0.85rem;
    flex-wrap: wrap;
    gap: 0.5rem;
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
    min-height: 300px;
    max-height: 600px;
    overflow-y: auto;
    padding: 0.85rem 1.25rem 1.25rem;
    box-sizing: border-box;
  }

  /* 预览区域滚动条样式 */
  .preview-content::-webkit-scrollbar {
    width: 6px;
  }

  .preview-content::-webkit-scrollbar-track {
    background: rgba(15, 23, 42, 0.2);
    border-radius: 10px;
  }

  .preview-content::-webkit-scrollbar-thumb {
    background: rgba(148, 163, 184, 0.3);
    border-radius: 10px;
  }

  .preview-content::-webkit-scrollbar-thumb:hover {
    background: rgba(148, 163, 184, 0.5);
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
    flex-wrap: wrap;
    gap: 0.5rem;
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

  /* 平板设备 */
  @media (max-width: 1024px) {
    .large-screen .layout {
      grid-template-columns: 280px 1fr;
    }

    aside {
      padding: 1.25rem;
    }

    .editor {
      padding: 1.5rem;
    }

    .tools {
      width: 100%;
      justify-content: flex-end;
    }
  }

  /* 小屏设备调整 */
  @media (max-width: 768px) {
    aside {
      padding: 1rem;
    }

    .editor {
      padding: 1rem;
    }

    .header h2 {
      font-size: 1.1rem;
    }

    .location-path {
      font-size: 0.75rem;
    }

    .editor-header {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .header-content h2 {
      font-size: 1.1rem;
    }

    .tools {
      width: 100%;
      justify-content: space-between;
    }

    .ghost,
    .danger {
      flex: 1;
      text-align: center;
      padding: 0.6rem 0.5rem;
      font-size: 0.85rem;
    }

    textarea {
      min-height: 200px;
    }

    .preview-card {
      margin-top: 1rem;
    }

    .preview-header {
      padding: 1rem 1rem 0.75rem;
    }

    .preview-content {
      min-height: 250px;
      max-height: 450px;
      padding: 0.75rem 1rem 1rem;
    }
  }

  /* 移动设备 - 小屏手机 */
  @media (max-width: 480px) {
    aside {
      padding: 0.75rem;
    }

    .header {
      gap: 0.5rem;
    }

    .header h2 {
      font-size: 1rem;
    }

    .primary {
      padding: 0.45rem 0.75rem;
      font-size: 0.9rem;
    }

    .search {
      padding: 0.55rem 0.75rem;
      font-size: 0.9rem;
    }

    .entry-list li button {
      padding: 0.7rem 0.75rem;
    }

    .entry-list h3 {
      font-size: 0.95rem;
    }

    .entry-list p {
      font-size: 0.78rem;
    }

    .editor {
      padding: 0.75rem;
      gap: 0.75rem;
    }

    .meta {
      font-size: 0.8rem;
    }

    .tools {
      gap: 0.4rem;
    }

    .ghost,
    .danger {
      padding: 0.5rem 0.4rem;
      font-size: 0.8rem;
    }

    .input-label {
      font-size: 0.9rem;
    }

    .title-input,
    textarea {
      padding: 0.65rem 0.85rem;
      font-size: 0.95rem;
    }

    textarea {
      min-height: 150px;
    }

    .preview-card {
      margin-top: 0.75rem;
    }

    .preview-header {
      padding: 0.85rem 0.85rem 0.65rem;
    }

    .preview-header h3 {
      font-size: 1rem;
    }

    .hint-text {
      font-size: 0.75rem;
    }

    .preview-content {
      min-height: 180px;
      max-height: 350px;
      padding: 0.65rem 0.85rem 0.85rem;
    }

    .footer-row {
      font-size: 0.82rem;
    }
  }

  /* 超小屏设备 */
  @media (max-width: 360px) {
    aside {
      padding: 0.5rem;
    }

    .location {
      padding: 0.5rem 0.6rem;
    }

    .editor {
      padding: 0.5rem;
    }

    .preview-header {
      padding: 0.65rem 0.65rem 0.5rem;
    }

    .preview-content {
      padding: 0.5rem 0.65rem 0.65rem;
    }

    .tools button {
      padding: 0.4rem 0.3rem;
      font-size: 0.75rem;
    }
  }

  /* 触摸设备优化 */
  @media (hover: none) and (pointer: coarse) {
    .primary,
    .ghost,
    .danger,
    .entry-list li button,
    .search,
    .title-input,
    textarea {
      -webkit-tap-highlight-color: transparent;
    }

    .entry-list li button {
      padding: 1rem 0.9rem;
    }

    .ghost,
    .danger {
      padding: 0.65rem 1rem;
    }
  }
</style>
