# NotepadRS 0.1 (Rust, egui)

A text and source code editor in the style of [Notepad++](https://github.com/notepad-plus-plus/notepad-plus-plus)
for Linux, shipped as a single AppImage.

Notepad++ itself is a pure Win32 application (C++, Win32 API, Scintilla) and cannot be ported
directly. NotepadRS is a **reimplementation in Rust** that follows the menu structure, commands,
shortcuts and file formats of Notepad++ – no code was taken from Notepad++. The toolbar uses the
Notepad++ *Fluent UI* icons (GPL-3.0, see [assets/toolbar](assets/toolbar/README.md)), recoloured
in the rust accent of NotepadRS.

User interface: **English** (default) and German – switchable under
*Settings → Preferences → General → Localization*.

## Features

Checked against the 339 menu commands of Notepad++ (`english.xml`). Every command is also available
in the *Shortcut Mapper* and can be bound freely.

| Menu | Contents |
|---|---|
| **File** | New/Open/Save/Save As/Save a Copy/Save All, Reload, Rename, Close (All/BUT Current/BUT Pinned/Left/Right/Unchanged), Move to Recycle Bin, Load/Save Session, Print (CUPS) / Print Now / to PostScript file, Open Containing Folder (file manager/terminal), Open Folder as Workspace, Open in Default Viewer, Recent Files, Restore Recent Closed File |
| **Edit** | Undo/Redo, Begin/End Select (also in column mode), date/time (short/long/custom format), Copy Path/Name/Dir/All Names, Indent, all case variants (incl. Proper/Sentence *blend*, iNVERT, ranDOm), all Line Operations (Duplicate, Delete, Split, Join, Move, Remove Duplicates/Empty, Insert Blank, 14 sort modes, Reverse, Randomize), Comment/Uncomment (line/block, per language), Auto-Completion incl. function parameter hints, EOL Conversion, Blank Operations, Paste Special (HTML/RTF content), On Selection (Open File, Redact █/●, Search on Internet), Multi-select All/Next (4 variants), Column Mode, Column Editor, Character Panel, Clipboard History, Read-Only (document/all/file attribute) |
| **Search** | Find/Replace/Mark with Normal/Extended/Regex, Find in Files (filters incl. `!*.bak` and `!+/folder`, subfolders, hidden folders) and Replace in Files, Find All (current/all documents), Search Results Window (F7) with F4/Shift+F4, Select and Find Next/Previous, Volatile Find, Incremental Search, Go to Line/Offset, Matching Brace, Select In-between, Change History, Style All/One Token with 5 styles + Clear/Jump/Copy Styled Text, Bookmarks (all operations incl. Paste to/Inverse), Find Characters in Range |
| **View** | Always on Top, Full Screen, Post-It, Distraction Free, Show Symbol (spaces/tabs, end of line, control characters, non-printing characters, indent guides, wrap symbol), Zoom, Move/Clone to Other View (split view), Move/Open in New Instance, tab commands (1–9, first/last, move, pin, 5 tab colours), horizontal, multi-line or vertical tab bar, Word Wrap, Focus on Another View, Hide Lines, Fold/Unfold All/Current/Level 1–8, Summary, Project Panels 1–3, Folder as Workspace, Document Map, Document List, Function List, Synchronize Vertical/Horizontal Scrolling, Monitoring (tail -f) |
| **Encoding** | ANSI, UTF-8, UTF-8-BOM, UTF-16 BE/LE BOM, ~35 character sets (Windows-125x, ISO-8859-x, KOI8, OEM 866, Shift-JIS, EUC-JP/KR, GBK, GB18030, Big5 …) – each with *interpret as* and *convert to*; mixed line endings are preserved |
| **Language** | ~220 languages (syntect/bat), A–Z menu, auto-detection; **User Defined Languages** with editor (keyword groups, comments, operators, delimiters, folding, styles) – **compatible with Notepad++ UDL XML** (import/export, folder `~/.config/notepad-rs/userDefineLangs/`) |
| **Settings** | Preferences (General, Editing, Margins/Border/Edge, New Document, Default Directory, Recent Files, Language/tab settings per language, Highlighting, Print, Searching, Backup, Auto-Completion, Multi-Instance & Date, Delimiter, Performance, Cloud & Link, Search Engine, MISC., File Association, Context Menu), Style Configurator (light/dark colour scheme, font, all UI colours), Shortcut Mapper (with conflict detection), import of `.tmTheme` colour schemes and `.sublime-syntax` languages, Edit Popup ContextMenu |
| **Tools** | MD5, SHA-1, SHA-256, SHA-512: from text, from files, from selection to clipboard |
| **Macro** | Record/playback, save with name and shortcut, run multiple times or until end of file, manage |
| **Run** | Run dialog with Notepad++ variables (`$(FULL_CURRENT_PATH)`, `$(CURRENT_WORD)`, `$(SYS.HOME)` …), saved commands with shortcuts |
| **Plugins** | Built-in equivalents of the most popular plugins: **Compare** (side by side, synchronised scrolling), **MIME Tools** (Base64, URL, Quoted-Printable), **NppExport** (export to HTML/RTF, HTML/RTF to clipboard), **Converter** (ASCII↔HEX), **JSON Tools**, **XML Tools** (Pretty Print, Linearize, syntax check) – plus **script plugins** (see below) |
| **Window / ?** | Windows dialog (sort, activate, save, close), Sort By (name, path, type, length, modified time), window list, command line help, Update NotepadRS, Set Updater Proxy, debug info, About |

Editor: custom virtualised widget (only visible lines are laid out and highlighted – smooth even
with files > 50 MB / 1 million lines), multiple carets (Ctrl+click), column selection (Alt+drag,
Alt+Shift+arrows), code folding (braces, indentation, tags, Markdown), fold, bookmark, line number
and change history margins, edge line, smart highlighting, matching braces/tags, auto-completion
(language keywords, words, paths), function parameter hints (Ctrl+Shift+Space or when typing `(`),
auto-closing of brackets/quotes/tags, clickable links, drag & drop of text (move/copy), overwrite
mode, session snapshot incl. unsaved changes, backup on save, detection of externally modified
files, single instance, minimize/close to the system tray.

### Script plugins

Executable files in `~/.config/notepad-rs/plugins/` appear under *Plugins → Scripts*.
They receive the selection (or the whole document if nothing is selected) on stdin; output on stdout
replaces it. Environment variables: `NRS_FILE`, `NRS_LANGUAGE`, `NRS_SELECTION`, `NRS_LINE`.

```bash
#!/bin/sh
# ~/.config/notepad-rs/plugins/Sort_unique  →  Plugins → Scripts → Sort unique
sort -u
```

### Function parameter hints

Signatures come from function definitions in the document (`fn`, `def`, `function`, C-style
declarations) and from **Notepad++ API files**: copy files such as `cpp.xml` or `php.xml` from the
`autoCompletion` folder of Notepad++ to `~/.config/notepad-rs/autoCompletion/`. They then provide
parameter hints with overloads and descriptions and extend function completion.

RTF on the clipboard (copy/paste) requires `wl-clipboard` (Wayland) or `xclip` (X11); HTML works
without additional tools.

### Updater

*? → Update NotepadRS* checks the [GitHub releases](https://github.com/StofflHD/notepad-rs/releases)
for a newer version; with *Settings → Preferences → MISC.* this also happens automatically at
startup (at most once a day). When NotepadRS runs as an AppImage, *Update now* downloads the new
AppImage, verifies its SHA-256 checksum (`.sha256` asset of the release) and replaces the AppImage
file in place; *Restart now* starts the new version. Downloads use `curl` (or `wget`), optionally
through the proxy from *? → Set Updater Proxy*. Before 1.0, pre-releases are offered as updates.

### System tray

*Settings → Preferences → MISC. → System tray*: minimize and/or close to the system tray, like
Notepad++. While the window is hidden, a tray icon (freedesktop StatusNotifierItem) restores it
with a click and offers *New*, *Open…* and *Exit*; *File → Exit* always quits. Supported by KDE
Plasma, Xfce, Cinnamon, LXQt, MATE and others; GNOME needs the *AppIndicator* extension.
`-systemtray` starts hidden in the tray.

### Command line

```
notepad-rs [-multiInst] [-nosession] [-ro] [-n<line>] [-c<column>] [-l<language>] [-udl="<name>"]
           [-alwaysOnTop] [-systemtray] [-notabbar] [-monitor] [-openSession <file>] [-r] [file …]
```

## Not implemented / different

- **Native Notepad++ plugins** (Windows DLLs) and *Plugins Admin* – built-in equivalents and script
  plugins instead.
- Right-to-left text direction (egui does not support bidirectional text rendering).
- File association uses a desktop entry and `mimeapps.list` instead of the Windows registry.

## Building

```bash
cargo test                          # 62 tests: core, editor widget, app flows (headless), translation, updater
cargo run --release -- file.txt     # run directly
packaging/build-appimage.sh         # -> dist/NotepadRS-<version>-x86_64.AppImage
```

The script downloads `appimagetool` when needed (or use `APPIMAGETOOL=/path/to/tool`).
The binary only depends on the glibc of the build system (here 2.39 → Debian 13, Ubuntu 24.04 and
newer). File dialogs use the XDG desktop portal (fallback: `zenity`), printing uses CUPS (`lp`).

Settings, session, UDLs, themes and plugins are stored in `~/.config/notepad-rs/`.

## Structure

| File | Contents |
|---|---|
| `src/doc.rs` | Document: text + line index, encoding, line endings (also mixed), undo/redo, multi-selection, folding/hidden lines, change history |
| `src/editor.rs` | Editor widget: input, caret, column selection, scrolling, margins, painting, auto-completion, parameter hints |
| `src/highlight.rs`, `src/fold.rs`, `src/udl.rs` | Syntax highlighting, fold structure, user defined languages |
| `src/search.rs`, `src/findfiles.rs` | Find/replace, find in files |
| `src/textops.rs`, `src/tools.rs` | Text/line operations; hashes, MIME/JSON/XML, compare, HTML/RTF export, printing, recycle bin, run, script plugins |
| `src/api.rs`, `src/assoc.rs`, `src/update.rs`, `src/tray.rs` | Function parameter hints (Notepad++ API XML), file association, updater, system tray |
| `src/cmd.rs`, `src/commands.rs` | Command registry (IDs, labels, shortcuts) and command execution |
| `src/app.rs`, `src/ui_*.rs` | Window, two views, session; menus, dialogs, preferences, panels |
| `src/theme.rs`, `src/icons.rs` | UI colour scheme (dark/light), recoloured Notepad++ toolbar icons |
| `src/i18n*.rs` | Translation (English → German) |
| `src/project.rs`, `src/settings.rs`, `src/ipc.rs`, `src/fonts.rs` | Project panels (Notepad++ workspace XML), settings, single instance, fonts |

## License

GPL-3.0-or-later, see [LICENSE](LICENSE).

---

© StofflHD and Claude
