import { invoke } from '@tauri-apps/api/core';
import { createPlatformInstanceService } from './platform/createPlatformInstanceService';
import type {
  CodexSessionVisibilityRepairSummary,
  CodexInstanceThreadSyncSummary,
  CodexSessionRecord,
  CodexSessionTrashSummary,
} from '../types/codex';

const service = createPlatformInstanceService('codex');

export const getInstanceDefaults = service.getInstanceDefaults;
export const listInstances = service.listInstances;
export const createInstance = service.createInstance;
export const updateInstance = service.updateInstance;
export const deleteInstance = service.deleteInstance;
export const startInstance = service.startInstance;
export const stopInstance = service.stopInstance;
export const closeAllInstances = service.closeAllInstances;
export const openInstanceWindow = service.openInstanceWindow;

export async function syncThreadsAcrossInstances(): Promise<CodexInstanceThreadSyncSummary> {
  return await invoke('codex_sync_threads_across_instances');
}

export async function repairSessionVisibilityAcrossInstances(): Promise<CodexSessionVisibilityRepairSummary> {
  return await invoke('codex_repair_session_visibility_across_instances');
}

export async function listSessionsAcrossInstances(): Promise<CodexSessionRecord[]> {
  return await invoke('codex_list_sessions_across_instances');
}

export async function moveSessionsToTrashAcrossInstances(
  sessionIds: string[],
): Promise<CodexSessionTrashSummary> {
  return await invoke('codex_move_sessions_to_trash_across_instances', { sessionIds });
}
