import { Channel, invoke } from '@tauri-apps/api/core';
import { ref } from 'vue';
import { desktop } from './bridge';

type UpdateStatus = { version: string | null; installable: boolean };
type InstallProgress =
  { kind: 'downloading'; received: number; total: number | null } | { kind: 'installing' };

export const releasesUrl = 'https://github.com/zzxCNCZ/oh-my-chatwise/releases';

export function useAppUpdates(prepareInstall: () => Promise<void>) {
  const status = ref<UpdateStatus>();
  const checking = ref(false);
  const installing = ref(false);
  const progress = ref<InstallProgress>();
  const error = ref('');

  async function check() {
    if (!desktop || checking.value || installing.value) return;
    checking.value = true;
    error.value = '';
    try {
      status.value = await invoke<UpdateStatus>('check_updates');
      return status.value;
    } catch (err) {
      error.value = `检查更新失败。${String(err)}`;
    } finally {
      checking.value = false;
    }
  }

  async function install() {
    const update = status.value;
    if (!update?.version || !update.installable || checking.value || installing.value) return;
    installing.value = true;
    error.value = '';
    progress.value = undefined;
    const channel = new Channel<InstallProgress>();
    channel.onmessage = (event) => (progress.value = event);
    try {
      await prepareInstall();
      await invoke('install_update', { version: update.version, progress: channel });
    } catch (err) {
      error.value = `安装更新失败。${String(err)}`;
      installing.value = false;
      progress.value = undefined;
    }
  }

  return { status, checking, installing, progress, error, check, install };
}
