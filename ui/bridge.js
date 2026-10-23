import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { readText } from '@tauri-apps/plugin-clipboard-manager';

window.downloader = {
  getStatus: () => invoke('get_status'),
  installYtDlp: async () => {
    try {
      return { ok: true, ...(await invoke('install_ytdlp')) };
    } catch (error) {
      const message = typeof error === 'string' ? error : error?.message || String(error);
      return { ok: false, error: message };
    }
  },
  chooseFolder: () => open({ directory: true, multiple: false }),
  openFolder: (folder) => invoke('open_folder', { folder }),
  openFfmpegPage: () => invoke('open_ffmpeg_page'),
  getClipboardText: () => readText(),
  download: (options) => invoke('start_download', options),
  cancelDownload: () => invoke('cancel_download'),
  onSetupProgress: (callback) => listen('setup-progress', (event) => callback(event.payload)),
  onDownloadProgress: (callback) => listen('download-progress', (event) => callback(event.payload)),
  onDownloadMessage: (callback) => listen('download-message', (event) => callback(event.payload)),
  onDownloadComplete: (callback) => listen('download-complete', (event) => callback(event.payload))
};
