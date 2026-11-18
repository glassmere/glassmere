# Glassmere Markup for VS Code

Syntax highlighting for Glassmere `.glass` views, and **Glassmere: Show
Preview** (also the preview button in the editor's title bar), which opens
the file in `glassmere-designer`.

The designer follows the file on disk: the preview changes on every save,
and with `"files.autoSave": "afterDelay"` as you type. Edits made in the
designer are written to the file and appear in the editor.

## Setup

1. `cargo install glassmere-designer` (or set `glassmere.designerPath`).
2. Copy or link this folder into `~/.vscode/extensions/glassmere-markup`, or
   package it with `npx @vscode/vsce package` and install the `.vsix`.
