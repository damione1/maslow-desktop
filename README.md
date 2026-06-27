# Maslow Desktop

![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)
![Svelte 5](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![FluidNC v1.21+](https://img.shields.io/badge/FluidNC-v1.21%2B-success)

A friendly control panel for the [Maslow CNC](https://www.maslowcnc.com/) running FluidNC, on **desktop, tablet and phone**.

It connects to your machine over the network and covers almost everything the built-in FluidNC web UI does, with two things it does better: it **respects the Maslow calibration state machine** so you can't push the machine into an invalid or stuck state, and it wraps calibration in a **plain-language guided wizard**.

![Toolpath preview on desktop](img/desktop-toolpath.png)

## Why another control panel?

- **State-machine aware.** Every action is gated against the firmware's allowed transitions (retract → extend → take slack → calibrate → ready to cut). No more wondering why the machine is stuck in an unknown state after a mis-step.
- **Guided calibration wizard.** Each step is explained in everyday language, advances automatically as the firmware reports progress, and offers a one-tap daily resume (just re-apply tension) plus release tension so the belts and frame can rest overnight.
- **One responsive app.** A real desktop build (macOS / Windows / Linux) and a touch-first layout for a tablet or phone mounted next to the machine, with manual control as the landing screen.
- **Almost the whole FluidNC web UI, re-implemented.** Jogging, jobs, SD card, settings and a raw console, with UX improvements layered on top (the guided wizard being the headline).

## Screenshots

| Manual control | Job & toolpath | Calibration | Settings |
| --- | --- | --- | --- |
| ![Manual control](img/mobile-control.png) | ![Job](img/mobile-job.png) | ![Setup](img/mobile-setup.png) | ![Settings](img/mobile-settings.png) |

![Calibration on desktop](img/desktop-calibrate.png)

## Download

Prebuilt installers are on the [**Releases**](https://github.com/damione1/maslow-desktop/releases/latest) page:

- **macOS** — `.dmg` (Apple Silicon & Intel), signed & notarized
- **Windows** — `.msi` or `.exe` installer

> An Android `.apk` will be added once mobile signing is set up.

## Getting started

You'll need a Maslow running **FluidNC** reachable on your network (by `maslow.local` or its IP).

> **Firmware compatibility:** built and tested against the Maslow build of **FluidNC v1.21 to v1.22+**. It degrades gracefully across versions: a few options (apply-tension limiting) require firmware **≥ v1.22** and are simply hidden or ignored on older builds.

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

### Develop

```bash
npm install
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

The packaged app lands in `src-tauri/target/release/`.

## Tech stack

[Tauri 2](https://tauri.app/) (Rust core) · [SvelteKit](https://kit.svelte.dev/) + [Svelte 5](https://svelte.dev/) · TypeScript. The frontend talks to FluidNC over WebSocket and HTTP; the Rust side owns the connection, job streaming and the calibration state model.

## Disclaimer

This is a community project and is **not affiliated with Maslow CNC or FluidNC**. CNC machines can cause injury and damage, so use at your own risk and keep the physical emergency stop within reach.

## License

Released under the MIT License.
