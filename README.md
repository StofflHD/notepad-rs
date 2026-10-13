# NotepadRS 0.1 (Rust, egui)

Text- und Quelltext-Editor im Stil von [Notepad++](https://github.com/notepad-plus-plus/notepad-plus-plus)
für Linux, als einzelnes AppImage.

Notepad++ selbst ist eine reine Win32-Anwendung (C++, Win32-API, Scintilla) und lässt sich nicht
direkt übertragen. NotepadRS ist eine **Neuimplementierung in Rust** mit Menüstruktur,
Befehlen, Tastenkürzeln und Dateiformaten von Notepad++ – es wurde kein Code aus Notepad++ übernommen.

Oberfläche: **Englisch** (Standard) und Deutsch – umschaltbar unter
*Settings → Preferences → General → Localization*.

## Funktionsumfang

Abgeglichen mit den 339 Menübefehlen von Notepad++ (`english.xml`). Alle Befehle stehen auch im
*Shortcut Mapper* und lassen sich frei belegen.

| Menü | Inhalt |
|---|---|
| **File** | New/Open/Save/Save As/Save a Copy/Save All, Reload, Rename, Close (All/BUT Current/BUT Pinned/Left/Right/Unchanged), Move to Recycle Bin, Load/Save Session, Print (CUPS) / Print Now / in PostScript-Datei, Open Containing Folder (Dateimanager/Terminal), Open Folder as Workspace, Open in Default Viewer, Recent Files, Restore Recent Closed File |
| **Edit** | Undo/Redo, Begin/End Select (auch Spaltenmodus), Datum/Uhrzeit (kurz/lang/eigenes Format), Copy Path/Name/Dir/All Names, Indent, alle Case-Varianten (inkl. Proper/Sentence *blend*, iNVERT, ranDOm), alle Line Operations (Duplicate, Delete, Split, Join, Move, Remove Duplicates/Empty, Insert Blank, 14 Sortierarten, Reverse, Randomize), Comment/Uncomment (Zeile/Block, je Sprache), Auto-Completion inkl. Funktionsparameter-Hinweisen, EOL Conversion, Blank Operations, Paste Special (HTML-/RTF-Inhalt), On Selection (Open File, Redact █/●, Search on Internet), Multi-select All/Next (4 Varianten), Column Mode, Column Editor, Character Panel, Clipboard History, Read-Only (Dokument/alle/Dateiattribut) |
| **Search** | Find/Replace/Mark mit Normal/Erweitert/Regex, Find in Files (Filter inkl. `!*.bak` und `!+/ordner`, Unterordner, versteckte Ordner) und Replace in Files, Find All (aktuelles/alle Dokumente), Search Results Window (F7) mit F4/Umschalt+F4, Select and Find Next/Previous, Volatile Find, Incremental Search, Go to Line/Offset, Matching Brace, Select In-between, Change History, Style All/One Token mit 5 Stilen + Clear/Jump/Copy Styled Text, Bookmarks (alle Operationen inkl. Paste to/Inverse), Find Characters in Range |
| **View** | Always on Top, Full Screen, Post-It, Distraction Free, Show Symbol (Leerzeichen/Tabs, Zeilenende, Steuerzeichen, nicht druckbare Zeichen, Einrückungshilfslinien, Umbruchsymbol), Zoom, Move/Clone to Other View (geteilte Ansicht), Move/Open in New Instance, Tab-Befehle (1–9, erster/letzter, verschieben, anheften, 5 Tab-Farben), horizontale, mehrzeilige oder vertikale Tab-Leiste, Word Wrap, Focus on Another View, Hide Lines, Fold/Unfold All/Current/Level 1–8, Summary, Project Panels 1–3, Folder as Workspace, Document Map, Document List, Function List, Synchronize Vertical/Horizontal Scrolling, Monitoring (tail -f) |
| **Encoding** | ANSI, UTF-8, UTF-8-BOM, UTF-16 BE/LE BOM, ~35 Zeichensätze (Windows-125x, ISO-8859-x, KOI8, OEM 866, Shift-JIS, EUC-JP/KR, GBK, GB18030, Big5 …) – jeweils *interpretieren als* und *konvertieren nach*; gemischte Zeilenenden bleiben erhalten |
| **Language** | ~220 Sprachen (syntect/bat), A–Z-Menü, Auto-Erkennung; **User Defined Languages** mit Editor (Schlüsselwortgruppen, Kommentare, Operatoren, Begrenzer, Faltung, Stile) – **kompatibel mit Notepad++-UDL-XML** (Import/Export, Ordner `~/.config/notepad-rs/userDefineLangs/`) |
| **Settings** | Preferences (General, Editing, Margins/Border/Edge, New Document, Default Directory, Recent Files, Language/Tab-Einstellungen je Sprache, Highlighting, Print, Searching, Backup, Auto-Completion, Multi-Instance & Date, Delimiter, Performance, Cloud & Link, Search Engine, MISC., File Association, Context Menu), Style Configurator (Farbschema hell/dunkel, Schrift, alle UI-Farben), Shortcut Mapper (mit Konflikterkennung), Import von `.tmTheme`-Farbschemata und `.sublime-syntax`-Sprachen, Edit Popup ContextMenu |
| **Tools** | MD5, SHA-1, SHA-256, SHA-512: aus Text, aus Dateien, aus Auswahl in die Zwischenablage |
| **Macro** | Aufzeichnen/Abspielen, speichern mit Namen und Tastenkürzel, mehrfach oder bis Dateiende ausführen, verwalten |
| **Run** | Run-Dialog mit Notepad++-Variablen (`$(FULL_CURRENT_PATH)`, `$(CURRENT_WORD)`, `$(SYS.HOME)` …), gespeicherte Befehle mit Tastenkürzel |
| **Plugins** | Eingebaute Entsprechungen der verbreitetsten Plugins: **Compare** (Seite an Seite, synchron scrollend), **MIME Tools** (Base64, URL, Quoted-Printable), **NppExport** (Export als HTML/RTF, HTML/RTF in die Zwischenablage), **Converter** (ASCII↔HEX), **JSON Tools**, **XML Tools** (Pretty Print, Linearize, Syntax-Check) – plus **Skript-Plugins** (siehe unten) |
| **Window / ?** | Windows-Dialog (sortieren, aktivieren, speichern, schließen), Sort By (Name, Pfad, Typ, Länge, Änderungszeit), Fensterliste, Kommandozeilenhilfe, Debug-Info, About |

Editor: eigenes, virtualisiertes Widget (nur sichtbare Zeilen werden gelayoutet/eingefärbt – flüssig
auch bei Dateien > 50 MB / 1 Mio. Zeilen), Mehrfach-Cursor (Strg+Klick), Spaltenauswahl (Alt+Ziehen,
Alt+Umschalt+Pfeile), Code-Faltung (Klammern, Einrückung, Tags, Markdown), Faltungs-, Lesezeichen-,
Zeilennummern- und Änderungsverlauf-Rand, Randlinie, Smart Highlighting, passende Klammern/Tags,
Autovervollständigung (Schlüsselwörter der Sprache, Wörter, Pfade), Funktionsparameter-Hinweise
(Strg+Umschalt+Leertaste bzw. beim Tippen von `(`), automatisches Schließen von
Klammern/Anführungszeichen/Tags, anklickbare Links, Text per Drag & Drop verschieben/kopieren,
Überschreibmodus, Sitzungs-Snapshot inkl. ungespeicherter Änderungen, Sicherung beim Speichern,
Erkennung extern geänderter Dateien, Einzelinstanz.

### Skript-Plugins

Ausführbare Dateien in `~/.config/notepad-rs/plugins/` erscheinen unter *Plugins → Scripts*.
Sie erhalten die Auswahl (oder ohne Auswahl das ganze Dokument) auf stdin; eine Ausgabe auf stdout
ersetzt sie. Umgebungsvariablen: `NRS_FILE`, `NRS_LANGUAGE`, `NRS_SELECTION`, `NRS_LINE`.

```bash
#!/bin/sh
# ~/.config/notepad-rs/plugins/Sort_unique  →  Plugins → Scripts → Sort unique
sort -u
```

### Funktionsparameter-Hinweise

Signaturen stammen aus den Funktionsdefinitionen im Dokument (`fn`, `def`, `function`, C-artige
Deklarationen) und aus **Notepad++-API-Dateien**: Dateien wie `cpp.xml` oder `php.xml` aus dem Ordner
`autoCompletion` von Notepad++ einfach nach `~/.config/notepad-rs/autoCompletion/` kopieren. Sie
liefern dann Parameter-Hinweise mit Überladungen und Beschreibung und erweitern die
Funktionsvervollständigung.

RTF in der Zwischenablage (Kopieren/Einfügen) benötigt `wl-clipboard` (Wayland) oder `xclip` (X11);
HTML funktioniert ohne Zusatzprogramme.

### Kommandozeile

```
notepad-rs [-multiInst] [-nosession] [-ro] [-n<Zeile>] [-c<Spalte>] [-l<Sprache>] [-udl="<Name>"]
           [-alwaysOnTop] [-notabbar] [-monitor] [-openSession <Datei>] [-r] [Datei …]
```

## Nicht umsetzbar / abweichend

- **Native Notepad++-Plugins** (Windows-DLLs) und *Plugins Admin* – stattdessen eingebaute
  Entsprechungen und Skript-Plugins.
- Rechts-nach-links-Textrichtung (egui beherrscht keine bidirektionale Textdarstellung).
- Windows-spezifische Einträge: Updater und Minimieren in den Tray. Die Dateizuordnung läuft
  stattdessen über Desktop-Eintrag und `mimeapps.list`.

## Bauen

```bash
cargo test                          # 56 Tests: Kern, Editor-Widget, App-Abläufe (headless), Übersetzung
cargo run --release -- datei.txt    # direkt starten
packaging/build-appimage.sh         # -> dist/NotepadRS-<version>-x86_64.AppImage
```

Das Skript lädt `appimagetool` bei Bedarf herunter (oder `APPIMAGETOOL=/pfad/zum/tool`).
Das Binary hängt nur von der glibc des Build-Systems ab (hier 2.39 → Debian 13, Ubuntu 24.04 und
neuer). Dateidialoge laufen über das XDG-Desktop-Portal (Fallback: `zenity`), Drucken über CUPS (`lp`).

Einstellungen, Sitzung, UDLs, Themes und Plugins liegen in `~/.config/notepad-rs/`.

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/doc.rs` | Dokument: Text + Zeilenindex, Kodierung, Zeilenenden (auch gemischt), Undo/Redo, Mehrfachauswahl, Faltung/ausgeblendete Zeilen, Änderungsverlauf |
| `src/editor.rs` | Editor-Widget: Eingabe, Cursor, Spaltenauswahl, Scrollen, Ränder, Zeichnen, Autovervollständigung |
| `src/highlight.rs`, `src/fold.rs`, `src/udl.rs` | Syntax-Highlighting, Faltstruktur, benutzerdefinierte Sprachen |
| `src/search.rs`, `src/findfiles.rs` | Suchen/Ersetzen, Suche in Dateien |
| `src/textops.rs`, `src/tools.rs` | Text-/Zeilenoperationen; Hashes, MIME/JSON/XML, Compare, HTML/RTF-Export, Druck, Papierkorb, Run, Skript-Plugins |
| `src/api.rs`, `src/assoc.rs` | Funktionsparameter-Hinweise (Notepad++-API-XML), Dateizuordnung |
| `src/cmd.rs`, `src/commands.rs` | Befehlsregister (IDs, Labels, Tastenkürzel) und Befehlsausführung |
| `src/app.rs`, `src/ui_*.rs` | Fenster, zwei Ansichten, Sitzung; Menüs, Dialoge, Einstellungen, Panels |
| `src/i18n*.rs` | Übersetzung (Englisch → Deutsch) |
| `src/project.rs`, `src/settings.rs`, `src/ipc.rs`, `src/fonts.rs` | Projekt-Panels (Notepad++-Workspace-XML), Einstellungen, Einzelinstanz, Schriften |

---

© StofflHD and Claude
