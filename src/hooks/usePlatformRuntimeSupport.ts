import { useMemo } from 'react';

export type PlatformRuntimeSupport = 'desktop' | 'macos-only';

export function usePlatformRuntimeSupport(mode: PlatformRuntimeSupport): boolean {
  return useMemo(() => {
    if (typeof navigator === 'undefined') return false;
    const platform = navigator.platform || '';
    const ua = navigator.userAgent || '';
    const isMac = /mac/i.test(platform) || /mac/i.test(ua);
    if (mode === 'macos-only') {
      return isMac;
    }
    const isWindows = /win/i.test(platform) || /windows/i.test(ua);
    const isLinux = /linux/i.test(platform) || /linux/i.test(ua);
    return isMac || isWindows || isLinux;
  }, [mode]);
}
