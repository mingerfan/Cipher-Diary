declare module './UnlockView.svelte' {
  import type { Component } from 'svelte';
  export interface UnlockViewProps {
    unlocked?: (payload: { created: boolean }) => void;
  }
  const UnlockView: Component<UnlockViewProps>;
  export default UnlockView;
}
