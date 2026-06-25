import {
  mountAntigravityRemote,
  unmountAntigravityRemote,
} from './shared';
import './style.css';

export async function mount(
  container: HTMLElement,
  hostApi: Parameters<typeof mountAntigravityRemote>[1],
) {
  return mountAntigravityRemote(container, hostApi);
}

export function unmount(container: HTMLElement) {
  unmountAntigravityRemote(container);
}
