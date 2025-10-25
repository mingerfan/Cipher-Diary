# Diary

Diary is a desktop-first encrypted journal powered by Tauri and SvelteKit. It stores every entry locally, derives your encryption key from a passphrase, and keeps the vault sealed until you unlock it within the running session.

DailyTxT inspired the user experience and initial Svelte structure. Their GPLv3 codebase can be found at [github.com/PhiTux/DailyTxT](https://github.com/PhiTux/DailyTxT).

## Features
- Create, edit, and delete encrypted diary entries with local AES-256-GCM encryption.
- Unlock the vault with a single passphrase derived via Argon2id.
- Search and filter entries instantly on the client.
- Runs cross-platform through the Tauri runtime.

## Getting Started
1. Install dependencies with your preferred package manager, for example `pnpm install`.
2. Start the development environment with `pnpm tauri dev`.
3. Build a production bundle with `pnpm tauri build`.

## License

This project is distributed under the terms of the **GNU General Public License v3.0 or later**. See the `LICENSE` file for the full text.

This repository reuses GPLv3-licensed frontend code from DailyTxT. In accordance with their license, this work remains GPLv3-compatible, and attribution is provided here.
