import { invoke } from '@tauri-apps/api/core';
import type { TopRightAdState } from '../types/topRightAd';

export async function getTopRightAdState(): Promise<TopRightAdState> {
  return await invoke('announcement_get_top_right_ad');
}
