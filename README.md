# Universal Downloader 1.0.2

A small Windows desktop interface for downloading media with yt-dlp.

## Install and use

1. Download and run `Universal-Downloader-Setup.exe` from the [latest GitHub release](https://github.com/randankevin004-hub/Universal-Downloader/releases/latest). The installer is per-user and does not require administrator privileges.
2. Open Universal Downloader. It checks for yt-dlp in the app's own tools folder and on your Windows PATH. If a copy is found, the app uses it. If no copy is found, choose **Set up** to allow the app to download the latest Windows executable from the official yt-dlp GitHub release. The app verifies the SHA-256 digest when GitHub provides one.
3. Paste a media link, choose a quality and save folder, then select **Download video**.

Downloads are saved locally. The app launches yt-dlp without a shell and passes the pasted URL as a separate argument. It uses an existing `ffmpeg.exe` on `PATH`; without FFmpeg, it selects a combined format when available and offers a link to FFmpeg builds. The FFmpeg location option is passed before the end-of-options marker so Windows paths are not interpreted as media URLs.

## Build from source

Requirements: Node.js/npm, Rust, Visual Studio C++ Build Tools with the Windows SDK, and WebView2 Runtime for running the app.

```powershell
npm install
npm run tauri -- build
```

The NSIS installer is generated in `src-tauri/target/release/bundle/nsis/`. The installer is unsigned, so Windows SmartScreen may show a warning for a new download.

## Download website

The public download page is published from `site/` with the GitHub Pages workflow in `.github/workflows/pages.yml`. The site links to the latest release installer. The download page explains first-run yt-dlp setup and the optional FFmpeg requirement.

Download the app at [universal downloader on GitHub Pages](https://randankevin004-hub.github.io/Universal-Downloader/) or browse the [source repository and releases](https://github.com/randankevin004-hub/Universal-Downloader).

## Project layout

- `ui/`: interface markup, styling, and client-side interactions
- `src-tauri/`: Rust desktop backend, Tauri configuration, and app icons
- `index.html`, `vite.config.js`, and `package.json`: frontend and build configuration

