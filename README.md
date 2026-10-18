# ovim (Edit Popup fork)

Edit any macOS text field in your own Neovim. Press a shortcut, edit with your
full config and plugins, then `:wq` to paste the result back.

This is a slimmed-down fork of [tonisives/ovim](https://github.com/tonisives/ovim)
that keeps only its Edit Popup. The system-wide Vim mode, click and scroll
modes, the floating indicator and its widgets, the CLI, the AeroSpace and bmux
integrations, live sync and auto-update are gone.

![Edit Popup](docs/images/edit-popup.gif)

## How it works

1. Focus a text field in any app and press **Ctrl+Opt+Cmd+E** (configurable).
2. ovim reads the field's text with the Accessibility API. In Safari, Chrome,
   Edge, Brave and Arc it first asks the page via JavaScript, which also gives
   the cursor position. If neither works it falls back to Cmd+A, Cmd+C.
3. Neovim opens in your terminal, in a window placed right below the field
   (above it if there is no room).
4. `:wq` pastes the saved text back into the field with Cmd+A, Cmd+V and then
   restores your clipboard. Quitting without saving (`:q!`, or closing the
   window) leaves the field untouched.

With Neovim, ovim also puts the cursor where it was in a browser field, moves
the browser cursor to where you left it in Neovim, and remembers the filetype
you set (`:set ft=markdown`) per website or app.

## Requirements

- macOS 10.15 or later
- [Neovim](https://neovim.io). Vim, Helix or another editor also work, without
  the cursor and filetype features.
- A terminal: [Alacritty](https://alacritty.org) (recommended), Kitty, WezTerm,
  Ghostty, iTerm2 or Terminal.app

## Build and install

You need [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org) 18+,
[pnpm](https://pnpm.io) and the Xcode Command Line Tools
(`xcode-select --install`).

```bash
git clone https://github.com/proever/ovim.git
cd ovim
pnpm install
pnpm tauri build --bundles app
cp -R src-tauri/target/release/bundle/macos/ovim.app /Applications/
```

The app keeps upstream ovim's bundle identifier (`com.tonis.ovim`) and settings
folder, so it replaces an installed upstream ovim. Quit that first.

### Keep the Accessibility permission across rebuilds

macOS ties the Accessibility permission to the app's code signature. An
unsigned build gets a new identity on every rebuild, and you have to remove and
re-add ovim under System Settings > Privacy & Security > Accessibility each
time. Signing with a certificate avoids that:

```bash
security find-identity -v -p codesigning   # list your certificates
APPLE_SIGNING_IDENTITY="Apple Development: Your Name (TEAMID)" pnpm package
```

`pnpm package` builds a signed app, verifies the signature and installs it as
`build/ovim.app` (set `OVIM_OUTPUT_DIR` to install elsewhere). Any code-signing
certificate works, including the free "Apple Development" one Xcode creates for
your Apple ID. Without `APPLE_SIGNING_IDENTITY` it uses your only "Developer ID
Application" certificate. `pnpm hooks:install` adds a Git hook that runs
`pnpm package` after merges, installing into `~/workspace/_tools` unless
`OVIM_OUTPUT_DIR` is set.

For development, `pnpm tauri dev` runs the app with live reload of the settings
window.

## First launch

ovim lives in the menu bar and has no Dock icon by default. On first launch it
asks for **Accessibility** access, which it needs to read the focused field and
to send Cmd+A, Cmd+C and Cmd+V. Click Allow and switch ovim on in the list. If
it isn't listed, drag it in from the helper window.

In browsers, macOS asks once per browser whether ovim may control it. Reading
the page also requires "Allow JavaScript from Apple Events" (Safari: Develop
menu; Chrome and other Chromium browsers: View > Developer). Without it ovim
still works through the fallbacks, but without cursor positioning, and
filetypes are remembered per browser instead of per site.

## Settings

Open **Settings...** from the menu bar icon, or open ovim again from Finder or
Spotlight.

- **Shortcut**: click it and press a new combination. It needs at least one
  modifier, except for function keys. If another app already owns the
  combination, the settings window says so.
- **Editor** and **Terminal**: which programs to use, with optional paths for
  installs that aren't found automatically.
- **Clipboard mode**: always use Cmd+A, Cmd+C to read the field instead of
  Accessibility or JavaScript. Try this for apps where capture misbehaves.
- **Saved filetypes**: review and delete remembered filetypes.
- **Window**: open as a popup next to the field, with its width and height.

Settings live in `~/Library/Application Support/ovim/settings.yaml`. Setting
`prewarm_terminal: true` under `nvim_edit` there (Alacritty only) keeps a hidden
editor running so the popup opens faster. Quit ovim before editing the file by
hand, since it rewrites the file when settings change.

## Troubleshooting

Logs are written to `/tmp/ovim-rust.log`. If the shortcut does nothing, check
that ovim has Accessibility access and look for a shortcut error on the Edit
Popup > Config tab. If text isn't pasted back, make sure you saved with `:w` or
`:wq`, and try clipboard mode for that app.

## License

MIT, see [LICENSE](LICENSE). Based on [ovim](https://github.com/tonisives/ovim)
by tonisives.
