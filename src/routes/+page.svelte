<script lang="ts">
  import MainView from '../lib/components/MainView.svelte';
  import UnlockView from '../lib/components/UnlockView.svelte';
  import { statusMessage, unlocked } from '../lib/stores/vault';
  import { onDestroy } from 'svelte';
  import { get } from 'svelte/store';

  let message = $state<string | null>(get(statusMessage));
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  const unsubscribe = statusMessage.subscribe((value) => {
    message = value;
  });

  $effect(() => {
    const current = message;
    if (!current) {
      if (toastTimer) {
        clearTimeout(toastTimer);
        toastTimer = null;
      }
      return;
    }

    if (toastTimer) {
      clearTimeout(toastTimer);
    }
    toastTimer = setTimeout(() => {
      statusMessage.set(null);
    }, 2800);
  });

  onDestroy(() => {
    unsubscribe();
    if (toastTimer) {
      clearTimeout(toastTimer);
      toastTimer = null;
    }
  });

  function handleUnlocked(payload: { created: boolean }) {
    if (!payload?.created) {
      statusMessage.set('欢迎回来');
    }
  }
</script>

{#if $unlocked}
  <MainView />
{:else}
  <UnlockView unlocked={handleUnlocked} />
{/if}

{#if message}
  <div class="toast" role="status">{message}</div>
{/if}

<style>
  :global(body) {
    margin: 0;
    font-family: 'Inter', system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #0f172a;
    color: #e2e8f0;
  }

  .toast {
    position: fixed;
    bottom: 32px;
    right: 32px;
    background: rgba(15, 23, 42, 0.9);
    border: 1px solid rgba(99, 102, 241, 0.4);
    color: #e2e8f0;
    padding: 0.9rem 1.1rem;
    border-radius: 12px;
    box-shadow: 0 18px 45px rgba(15, 23, 42, 0.55);
    backdrop-filter: blur(8px);
    animation: fade-in 180ms ease-out;
    max-width: 360px;
  }

  @keyframes fade-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
