import * as bridge from './bridge';
import type { Theme } from './types';

export class ThemeSession {
  current = $state<Theme>('light');
  ready = $state(false);
  busy = $state(false);
  error = $state('');

  connect(): () => void {
    let disposed = false;
    let off: (() => void) | undefined;
    let changeCount = 0;
    const apply = (theme: Theme) => {
      if (disposed) return;
      this.current = theme;
      document.documentElement.dataset.theme = theme;
    };
    const onChange = (value: unknown) => {
      if (value !== 'light' && value !== 'dark') return;
      changeCount++;
      apply(value);
    };
    const onStorage = (event: StorageEvent) => {
      if (event.key === 'bullet-demo-theme') onChange(event.newValue === 'dark' ? 'dark' : 'light');
    };
    if (!bridge.native) window.addEventListener('storage', onStorage);
    void (async () => {
      try {
        const unsubscribe = await bridge.subscribe('theme-changed', onChange);
        if (disposed) { unsubscribe(); return; }
        off = unsubscribe;
        const beforeLoad = changeCount;
        const theme = await bridge.getTheme();
        if (beforeLoad === changeCount) apply(theme);
      } catch (error) {
        if (!disposed) this.error = String(error);
      } finally {
        if (!disposed) this.ready = true;
      }
    })();
    return () => { disposed = true; off?.(); window.removeEventListener('storage', onStorage); };
  }

  async toggle(): Promise<void> {
    if (!this.ready || this.busy) return;
    this.busy = true;
    this.error = '';
    try {
      this.current = await bridge.setTheme(this.current === 'light' ? 'dark' : 'light');
      document.documentElement.dataset.theme = this.current;
    } catch (error) {
      this.error = String(error);
    } finally {
      this.busy = false;
    }
  }
}
