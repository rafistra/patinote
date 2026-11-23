# Keys

Patinote is driven from the keyboard. The editor follows Vim (Neovim is the
reference for how each command behaves), and the rest of the app uses the
same habits: `j`/`k` to move, `Enter` to open, `Esc` to go back.

`<leader>` is `Space` in normal mode. Press it and wait: after 300 ms a popup
lists the keys that may follow.

## Copy and paste

`y` and `p` use the system clipboard, like `clipboard=unnamedplus` in Neovim.
There is no `Ctrl+C`/`Ctrl+V` in the editor.

| Keys | What it does |
| --- | --- |
| `yy`, `3yy` | Copy the line, or 3 lines |
| `yw`, `y$`, `yiw`, `Y` | Copy a word, to the end of the line, the word under the cursor, to the end of the line |
| `v` … `y` | Copy the selection |
| `V` … `y` | Copy whole lines |
| `dd`, `d{motion}`, `x`, `v` … `d` | Cut: the text is deleted and copied |
| `p` / `P` | Paste after / before the cursor (below / above the line for whole lines) |
| `v` … `p` | Replace the selection with the clipboard |

Text copied in another app is read when the Patinote window gains focus; then
press `p` in normal mode. In insert mode, press `Esc` first.

## Editor (Vim)

### Modes

| Keys | What it does |
| --- | --- |
| `Esc`, `Ctrl+[`, `Ctrl+c` | Back to normal mode |
| `i` `a` `I` `A` `o` `O` | Insert before / after the cursor, at the start / end of the line, on a new line below / above |
| `s` `S` | Replace the character / the line, then insert |
| `v` / `V` | Visual / visual line |
| `o` (visual) | Go to the other end of the selection |

Set `insert_escape = "jk"` under `[editor]` in
`~/.config/patinote/config.toml` to leave insert mode by typing `jk`.

### Moving

| Keys | What it does |
| --- | --- |
| `h` `j` `k` `l`, arrows | Left, down, up, right |
| `w` `b` `e`, `W` `B` `E` | Next word, previous word, end of word (capitals: words separated by spaces only) |
| `0` `^` `$`, `Home` `End` | Start of line, first non-blank, end of line |
| `gg` `G`, `{n}G` | First line, last line, line `n` |
| `Ctrl+d` `Ctrl+u` | Half a screen down / up |
| `f{c}` `F{c}` `t{c}` `T{c}` | To the next / previous `c` on the line, or just before it |
| `;` `,` | Repeat the last `f`/`t`, forward / backward |
| `/` `?`, `n` `N` | Search forward / backward, next / previous match |
| `Ctrl+o` `Ctrl+i` | Back / forward through jumps, across notes |

A number before a command repeats it: `5j`, `3dd`, `2w`.

### Editing

| Keys | What it does |
| --- | --- |
| `d` `c` `y` + motion | Delete, change, copy: `dw`, `c$`, `y3j`, `dtx` |
| `dd` `cc` `yy`, `D` `C` `Y` | The same on the whole line, or to the end of the line |
| `iw` `aw`, `i"` `a"`, `i'` `a'`, `i(` `a(`, `i[` `a[` | Text objects: `ciw`, `di"`, `ya(` |
| `x` `X` | Delete the character under / before the cursor |
| `r{c}` | Replace the character with `c` |
| `J` | Join with the next line |
| `>>` `<<` | Indent / unindent |
| `u`, `Ctrl+r` | Undo, redo |
| `.` | Repeat the last change |
| `Ctrl+w`, `Ctrl+u` (insert) | Delete the word / to the start of the line |

### Links

| Keys | What it does |
| --- | --- |
| `[[` (insert) | Suggest notes; `Ctrl+n`/`Ctrl+p` or arrows to pick, `Enter` or `Tab` to insert, `Esc` to close |
| `gd`, `Enter` on a link | Follow the link |
| `gx` | Open the link in the default app |
| `Ctrl`+click | Follow the link under the mouse |

## Notes and buffers

| Keys | What it does |
| --- | --- |
| `<leader>f`, `Ctrl+Shift+O` | Find a note (quick switcher) |
| `<leader>n` | New note |
| `<leader>x` | Close the note |
| `gt` `gT`, `]b` `[b` | Next / previous open note |

In the quick switcher: type to filter, `Ctrl+n`/`Ctrl+j`/`↓` and
`Ctrl+p`/`Ctrl+k`/`↑` to move, `Enter` to open, `Shift+Enter` to create a
note with the typed name, `Esc` to close.

## Panels and layout

| Keys | What it does |
| --- | --- |
| `<leader>e` | Explorer |
| `<leader>b` | Backlinks |
| `<leader>o` | Outline |
| `<leader>r` | Reading mode |
| `<leader>c` | Sync conflicts |
| `<leader>l` | Next layout: minimal, classic, focus |
| `Ctrl+w h`, `Ctrl+w l` | Move the focus to the panel on the left / right |
| `Ctrl+=` `Ctrl+-` `Ctrl+0` | Larger text, smaller text, default size |

### Explorer

| Keys | What it does |
| --- | --- |
| `j` `k`, arrows | Move |
| `l`, `Enter` | Open the note or folder |
| `h` | Close the folder, or go to the folder above |
| `a`, `A` | New note, new folder |
| `r`, `m`, `d` | Rename, move, delete (to `.trash/`) |
| `Esc` | Back to the editor |

### Backlinks, outline, lists and dialogs

`j`/`k` or arrows to move, `Enter` to open, `Esc` to close. Lists that take
typed text (dialogs, the switcher) use `Ctrl+n`/`Ctrl+p` or arrows instead of
`j`/`k`.

### Reading mode

| Keys | What it does |
| --- | --- |
| `j` `k`, `Ctrl+d` `Ctrl+u` | Scroll a line, half a screen |
| `gg` `G` | Top, bottom |
| `Tab` `Shift+Tab` | Next / previous link |
| `Enter` | Follow the link |
| `<leader>r` | Back to editing |

## Commands

Type `:` in normal mode.

| Command | What it does |
| --- | --- |
| `:w` | Save now (autosave runs anyway) |
| `:q`, `:q!` | Close the note; `!` drops changes since the last save. Quits when no note is open |
| `:wq`, `:x` | Save and close |
| `:qa`, `:qa!` | Quit |
| `:e <note>` | Open a note by name, alias or path |
| `:bn`, `:bp`, `:b <n or name>`, `:bd` | Next, previous, pick, close a buffer |
| `:layout [minimal\|classic\|focus]` | Change the layout, or show the current one |
| `:set rnu`, `:set nornu` | Relative line numbers on / off |
| `:conflicts` | List sync conflicts |

## Not yet

Named registers, macros, visual block (`Ctrl+v`), `:s` and a key map file
come later. Pasting with `Ctrl+Shift+V` in insert mode and selecting text with
the mouse are not supported.
