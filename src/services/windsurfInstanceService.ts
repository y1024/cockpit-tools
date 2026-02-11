import { invoke } from '@tauri-apps/api/core';
import { InstanceDefaults, InstanceProfile } from '../types/instance';

export async function getInstanceDefaults(): Promise<InstanceDefaults> {
  return await invoke('windsurf_get_instance_defaults');
}

export async function listInstances(): Promise<InstanceProfile[]> {
  return await invoke('windsurf_list_instances');
}

export async function createInstance(payload: {
  name: string;
  userDataDir: string;
  extraArgs?: string;
  bindAccountId?: string | null;
  copySourceInstanceId: string;
  initMode?: 'copy' | 'empty';
}): Promise<InstanceProfile> {
  return await invoke('windsurf_create_instance', {
    name: payload.name,
    userDataDir: payload.userDataDir,
    extraArgs: payload.extraArgs ?? '',
    bindAccountId: payload.bindAccountId ?? null,
    copySourceInstanceId: payload.copySourceInstanceId,
    initMode: payload.initMode ?? 'copy',
  });
}

export async function updateInstance(payload: {
  instanceId: string;
  name?: string;
  extraArgs?: string;
  bindAccountId?: string | null;
  followLocalAccount?: boolean;
}): Promise<InstanceProfile> {
  const body: Record<string, unknown> = {
    instanceId: payload.instanceId,
  };
  if (payload.name !== undefined) {
    body.name = payload.name;
  }
  if (payload.extraArgs !== undefined) {
    body.extraArgs = payload.extraArgs;
  }
  if (payload.bindAccountId !== undefined) {
    body.bindAccountId = payload.bindAccountId;
  }
  if (payload.followLocalAccount !== undefined) {
    body.followLocalAccount = payload.followLocalAccount;
  }
  return await invoke('windsurf_update_instance', body);
}

export async function deleteInstance(instanceId: string): Promise<void> {
  return await invoke('windsurf_delete_instance', { instanceId });
}

export async function startInstance(instanceId: string): Promise<InstanceProfile> {
  return await invoke('windsurf_start_instance', { instanceId });
}

export async function stopInstance(instanceId: string): Promise<InstanceProfile> {
  return await invoke('windsurf_stop_instance', { instanceId });
}

export async function closeAllInstances(): Promise<void> {
  return await invoke('windsurf_close_all_instances');
}

export async function openInstanceWindow(instanceId: string): Promise<void> {
  return await invoke('windsurf_open_instance_window', { instanceId });
}
