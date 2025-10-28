import type { Component } from 'svelte';

export interface UnlockViewProps {
  unlocked?: (payload: { created: boolean }) => void;
}

declare const UnlockView: Component<UnlockViewProps>;
export default UnlockView;
