# 🐾 !StayAlone

> **English** · [Português](README.pt-BR.md)

A pixel-art pet that keeps you company on your Windows desktop. It walks along the
taskbar, naps when you step away, reminds you to drink water and stretch, and you
can chat with it through an AI plugin. Built to be **as light as possible**, straight
on the Win32 API, with no UI framework.

> The interface is available in **English** and **Brazilian Portuguese** (it follows Windows,
> or pick one in Settings). The code comments are in Portuguese.

<p align="center">
  <img src="docs/mascots.png" alt="The four built-in mascots: Calcifer the cat, Lance the dog, Zezé the bunny and Jujubs the dinosaur" width="80%">
</p>

| | Target | Measured |
|---|---|---|
| Executable | < 5 MB | ~750 KB — a single `.exe`, AI chat included |
| Private memory | < 20 MB | ~2–4 MB |
| CPU | ~0% | ~0.03% of the machine |

## Download

Grab **[`dontStayAlone.exe`](https://github.com/4TyllaL/notStayAlone/releases/latest)** from the
latest release and run it (Windows 10/11, 64-bit). That single file is the whole app, AI chat
included. No installer, nothing written outside `%APPDATA%\StayAlone`.

> Why *dont*StayAlone? The app is **!StayAlone**, but GitHub strips the `!` from file names.

Code signing through the SignPath Foundation is being set up (see
[Code signing policy](#code-signing-policy)); until the first signed release, Windows
SmartScreen may warn on first run (*More info → Run anyway*). GitHub shows the file's SHA-256
next to the download, and every release built by GitHub Actions carries a
[build provenance attestation](https://github.com/4TyllaL/notStayAlone/attestations).

After that, the app **updates itself**: once a day it checks the latest GitHub release, and the
panel offers the new version. The download is verified against the release's SHA-256 before it
replaces the `.exe` (you can turn this off in Settings).

## Features

- **Four mascots:** Calcifer (cat), Lance (dog), Zezé (bunny) and Jujubs (dinosaur),
  each with their own lines. Lines switch to the feminine form for "she" mascots.
- **A buddy on screen:** pick a second mascot that walks around, visits the first one and
  chases the ball too.
- **Companionship:** notices when you leave and come back, gentle reminders counted only
  while you actually use the PC (water, stretch, rest your eyes, plus your own), affection
  hearts, a daily summary and an optional focus timer (pomodoro). Log each glass of water
  from the panel (or `--agua`) so it shows up in the summary.
- **Routine and mood:** birthday wishes, Monday and Friday lines, a nudge after three hours
  without a break, and it stays **quiet during meetings** (Teams, Zoom, Webex... — it only
  checks the name of the program in front, never the screen).
- **Play:** drag and throw it around, give it a treat, play ball. Reacts to low battery
  and to long typing streaks. Hides itself during full-screen games, videos and presentations.
- **Mascot maker:** draw one pose and the app generates every animation (blink, sleep,
  walk, fall, happy). Optionally draw your own sleeping, eating, happy and walking poses
  (with onion skin). Mascots can be 16×16 or 32×32 — or describe one and let the AI draw it.
- **Chat:** any OpenAI-compatible API (Gemini by default, OpenAI, OpenRouter, local Ollama).
- **Memory:** it can remember what you tell it in chat (an exam on Friday, your cat's name)
  and bring it up later. Kept only in a text file on your PC that you can view, edit or
  wipe; passwords, documents and numbers are never stored.
- **Community mascots:** install mascots shared in this repository, right from Settings.
  Only mascots — drawings and lines in plain text, nothing that runs on your PC — and every
  file is checked against its SHA-256 before it's written.
- **Dark mode:** follows Windows, or choose light/dark in Settings.
- **Plugins:** your own `.exe` or PowerShell scripts that make the mascot say things on a
  schedule, or that answer the chat. Toggled in the settings, verified by SHA-256.
- **Mods:** mascots are plain-text files, so you can make and share your own.

## Screenshots

<p align="center">
  <img src="docs/panel.png" alt="Mascot panel: quick actions, mascot picker, size selector and toggles" width="30%">
  <img src="docs/settings-general.png" alt="Settings: sidebar with the current mascot and cards" width="66%">
</p>
<p align="center">
  <img src="docs/settings-maker.png" alt="Mascot maker: 16x16 pixel editor, palette and AI drawing" width="49%">
  <img src="docs/settings-plugins.png" alt="Plugins page: installed plugins you can switch on and off" width="49%">
</p>

## How it stays light

- Native layered windows (`UpdateLayeredWindow`); no WebView, no UI framework.
- Redraws only when the frame changes; every timer runs only when needed (60 fps only while
  something is falling, 10 fps normally, ~1.4 fps asleep, nothing while hidden).
- The panel, settings, speech bubble and toys exist only while on screen.
- No global keyboard/mouse hooks: it only knows *when* there was input
  (`GetLastInputInfo`), never *what*.
- The mascot process never touches the network. When you chat, check for updates or open
  the gallery, the app starts a second, short-lived copy of itself (`--ia`,
  `--procurar-versao`, `--galeria`...) that makes the request and exits.

## Security

Reviewed in the style of a Common Criteria Security Target — threats, assumptions and how
each is handled are in [`SECURITY.md`](SECURITY.md) (in Portuguese). Highlights:

- The API key is stored in the **Windows Credential Manager** (DPAPI), never in a file or
  environment variable; it only travels over **HTTPS** (TLS 1.2+), redirects disabled.
- No pointers in window messages: data between windows goes through an in-process mailbox,
  so forged messages from other programs are ignored.
- Every input is bounded (mod files, AI replies, JSON depth, plugin output and run time).
- New plugins start **off**; enabling one asks for confirmation and pins its SHA-256.
- Updates only come from this repository's releases, are checked against the SHA-256
  GitHub publishes, and never go back to an older version. Gallery files are checked
  against the SHA-256 in `gallery/index.json`.
- `winhttp.dll`, `dwmapi.dll` and `uxtheme.dll` are loaded from System32 only; binaries have
  ASLR and DEP and run as the regular user (`asInvoker`). The MSVC build adds Control Flow
  Guard and CET shadow stack compatibility.

## Build

Requires Rust (`stable-x86_64-pc-windows-gnu` or MSVC toolchain). With MSVC,
`.cargo/config.toml` turns on Control Flow Guard and a static CRT; the GitHub Actions
workflows build, test and publish releases that way.

```bash
cargo build --release
```

This produces `target/release/dontStayAlone.exe` — a single file with the four mascots, the
AI chat, the icon and the manifest embedded. The icon is drawn from the Calcifer sprite at
build time (`build.rs`), no external tools needed.

```bash
cargo test
```

## Usage

- **Click** the mascot to pet it; **drag** to carry and throw it.
- **Right-click** it (or click the tray icon) to open the panel: treat, ball, chat, focus
  timer, switch mascot, size, silence, settings.
- Command line (handy for Windows shortcuts; works while the app is running):
  `dontStayAlone.exe --bolinha` (ball), `--petisco` (treat), `--conversar` (chat),
  `--agua` (I drank water), `--resumo` (daily summary), `--esconder` (hide/show),
  `--configurar` (settings).

Settings, mods and plugins live in `%APPDATA%\StayAlone\`. Mod and plugin formats are
documented in [`README.pt-BR.md`](README.pt-BR.md) and in the `LEIA-ME.txt` files the app
creates in those folders.

## Project layout

```
src/            the app (Win32 + pure logic modules with tests)
src/settings/   settings window and pixel editor
src/ai/         AI chat (OpenAI-compatible API over WinHTTP, minimal JSON), run as --ia
assets/         mascots, props, lines (Portuguese and *_en.txt) and the example plugin
gallery/        community mascots (index.json made by tools/gallery.py)
build.rs        icon, manifest and version resources
docs/           screenshots
```

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).

- Only `dontStayAlone.exe` built by the [Release workflow](.github/workflows/release.yml) from a
  version tag of this repository is signed. Nothing built on a personal machine is signed.
- Committers and reviewers: [@4TyllaL](https://github.com/4TyllaL)
- Approvers: [@4TyllaL](https://github.com/4TyllaL) — every signing request is approved by hand.

**Privacy:** this program does not send any information to other networked systems unless
the user asks for it, with these exceptions, all described in [`SECURITY.md`](SECURITY.md):
the daily update check against this repository's GitHub releases (can be turned off in
Settings), the AI chat with the provider the user configures, and the community gallery
when the user opens it. There is no telemetry.

## License

[MIT](LICENSE)
