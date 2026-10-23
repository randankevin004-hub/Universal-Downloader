import './bridge.js';

const $ = (selector) => document.querySelector(selector);
const form = $('#downloadForm');
const urlInput = $('#urlInput');
const folderButton = $('#folderButton');
const folderText = $('#folderText');
const toolStatus = $('#toolStatus');
const setupPanel = $('#setupPanel');
const progressArea = $('#progressArea');
const errorMessage = $('#errorMessage');
const downloadButton = $('#downloadButton');
const buttonText = $('#buttonText');
const progressTitle = $('#progressTitle');
const progressPercent = $('#progressPercent');
const progressFill = $('#progressFill');
const progressDetail = $('#progressDetail');
let selectedFolder = null;
let busy = false;

function showError(message) {
  errorMessage.textContent = message;
  errorMessage.classList.remove('hidden');
}
function clearError() { errorMessage.classList.add('hidden'); errorMessage.textContent = ''; }
function setBusy(value) {
  busy = value;
  downloadButton.disabled = value;
  buttonText.textContent = value ? 'Working…' : 'Download video';
}
function updateFolderLabel(folder) {
  if (!folder) { folderText.textContent = 'Downloads'; return; }
  folderText.textContent = folder.replace(/\\/g, '/').split('/').filter(Boolean).pop() || folder;
  folderButton.title = folder;
}
function updateStatus(status) {
  const label = toolStatus.querySelector('span');
  $('#ffmpegNote').classList.toggle('hidden', Boolean(status.ffmpeg));
  if (status.ytDlp) {
    toolStatus.className = 'tool-status ready';
    const source = status.ytDlpManaged ? 'app copy' : 'found on your PC';
    label.textContent = status.version ? `Ready · yt-dlp ${status.version} · ${source}${status.ffmpeg ? '' : ' · standard quality'}` : `Downloader ready · ${source}`;
    setupPanel.classList.add('hidden');
  } else {
    toolStatus.className = 'tool-status missing';
    label.textContent = 'Setup needed';
    setupPanel.classList.remove('hidden');
  }
  if (status.downloadFolder) {
    if (!selectedFolder) selectedFolder = status.downloadFolder;
    updateFolderLabel(selectedFolder);
  }
}

window.downloader.onSetupProgress(({ message, percent }) => {
  const button = $('#installButton');
  button.disabled = true;
  button.textContent = `${Math.round(percent)}%`;
  setupPanel.querySelector('strong').textContent = message;
});
window.downloader.onDownloadProgress(({ percent, message }) => {
  progressTitle.textContent = percent >= 100 ? 'Finishing your video…' : 'Downloading your video…';
  progressPercent.textContent = `${Math.round(percent)}%`;
  progressFill.style.width = `${Math.min(100, Math.max(0, percent))}%`;
  progressDetail.textContent = message || 'This can take a little while.';
});
window.downloader.onDownloadMessage((message) => { progressDetail.textContent = message.slice(0, 150); });
window.downloader.onDownloadComplete(({ ok, cancelled, message }) => {
  progressTitle.textContent = message;
  progressPercent.textContent = ok ? '100%' : '—';
  if (ok) progressFill.style.width = '100%';
  if (cancelled) progressDetail.textContent = 'You can start another download whenever you’re ready.';
});

$('#pasteButton').addEventListener('click', async () => {
  try {
    const text = await window.downloader.getClipboardText();
    if (!text) return showError('There is no link on the clipboard yet. Copy a video link, then try again.');
    urlInput.value = text.trim();
    urlInput.focus();
  }
  catch { showError('Clipboard access is unavailable. Paste the link with Ctrl+V.'); }
});
folderButton.addEventListener('click', async () => {
  const folder = await window.downloader.chooseFolder();
  if (folder) { selectedFolder = folder; updateFolderLabel(folder); }
});
$('#openFolderButton').addEventListener('click', () => window.downloader.openFolder(selectedFolder));
$('#ffmpegLink').addEventListener('click', () => window.downloader.openFfmpegPage());

$('#installButton').addEventListener('click', async () => {
  clearError();
  const button = $('#installButton');
  button.disabled = true;
  button.textContent = 'PLEASE WAIT';
  const result = await window.downloader.installYtDlp();
  if (!result.ok) {
    button.disabled = false;
    button.textContent = 'TRY AGAIN';
    showError(result.error || 'Setup could not be completed. Check your internet connection and try again.');
  } else {
    setupPanel.querySelector('strong').textContent = 'All set. yt-dlp is installed.';
    updateStatus(result);
  }
});

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  clearError();
  if (busy) return;
  const current = await window.downloader.getStatus();
  if (!current.ytDlp) {
    showError('Please set up yt-dlp first. Your consent is required before it is downloaded.');
    setupPanel.classList.remove('hidden');
    return;
  }
  setBusy(true);
  progressArea.classList.remove('hidden');
  progressTitle.textContent = 'Starting your download…';
  progressPercent.textContent = '0%';
  progressFill.style.width = '0%';
  progressDetail.textContent = 'Connecting to the source…';
  const result = await window.downloader.download({ url: urlInput.value.trim(), quality: $('#qualitySelect').value, folder: selectedFolder });
  setBusy(false);
  if (result.needsSetup) {
    setupPanel.classList.remove('hidden');
    showError(result.error);
  } else if (!result.ok && !result.cancelled) showError(result.error || 'The download could not be completed.');
  else {
    progressTitle.textContent = 'Your video is ready.';
    progressDetail.textContent = `Saved to ${selectedFolder || 'Downloads'}`;
  }
});

$('#cancelButton').addEventListener('click', async () => {
  await window.downloader.cancelDownload();
  setBusy(false);
  progressTitle.textContent = 'Download cancelled';
  progressDetail.textContent = 'You can start another download whenever you’re ready.';
});

window.downloader.getStatus().then(updateStatus).catch(() => {
  toolStatus.className = 'tool-status missing';
  toolStatus.querySelector('span').textContent = 'Unable to check setup';
});
