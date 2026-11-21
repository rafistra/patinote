# 0003 — Design system UI (Fase 2)

- Status: **diterima** (20 November 2026)
- Terkait PRD: F-01, F-03, F-04, F-05, F-06, F-07, F-08, F-09, F-10, F-11, F-27, F-28
- Acuan visual: `docs/mockups/*.html` (buka di browser dari dalam repo) dan screenshot referensinya di `docs/mockups/png/`. Salinan statis dari kanvas mockup di claude.ai.
- Isi contoh di mockup tersedia sebagai vault sintetis di `fixtures/vault-mockup/`.

Dokumen ini adalah sumber angka untuk tampilan: warna, huruf, ukuran, dan jarak. Mockup menunjukkan susunan; dokumen ini menentukan nilainya. Bila mockup dan dokumen ini berbeda, **dokumen ini yang benar**.

## 1. Cara memakai di kode

- Semua nilai di sini menjadi konstanta di `crates/patinote-app/src/theme/tokens.rs`. Tidak ada warna, ukuran huruf, atau jarak yang ditulis langsung di kode widget.
- Nama konstanta mengikuti nama token, ditulis `SCREAMING_SNAKE_CASE`: `bg.app` menjadi `BG_APP`, `text.muted` menjadi `TEXT_MUTED`.
- Gaya widget Iced (`button::Style`, `container::Style`, dan seterusnya) dibuat di `theme/styles.rs` dari token, bukan dari angka.
- Fase 2 hanya punya tema gelap. Struktur `Theme` tetap dibuat agar tema terang (Fase 8) cukup menambah satu set token.

## 2. Mockup

| File | Isi | Kebutuhan |
| --- | --- | --- |
| `01-minimal.html` | Mode minimal (default), panel tertutup | F-05, F-28 |
| `02-minimal-panels-open.html` | Mode minimal dengan explorer, panel samping, dan popup leader terbuka | F-09, F-28 |
| `03-classic.html` | Mode classic | F-28 |
| `04-focus.html` | Mode focus | F-28 |
| `05-quick-switcher.html` | Quick switcher | F-07 |
| `06-link-autocomplete.html` | Autocomplete `[[` | F-08 |
| `07-reading-mode.html` | Mode baca (1440×1180) | F-27 |
| `08-external-change-conflict.html` | Banner dan diff perubahan dari luar | F-11 |
| `09-rename-delete-dialogs.html` | Dialog ganti nama dan hapus | F-10 |
| `10-welcome-and-missing-vault.html` | Pertama kali dibuka (belum ada vault) dan vault terakhir tidak ditemukan | F-01 |
| `11-no-note-open.html` | Tanpa catatan terbuka: catatan terakhir dan pintasan | F-28 |
| `12-ambiguous-link.html` | `gd` pada link ke dua catatan bernama sama, penanda link ambigu | F-08 |
| `13-visual-mode-and-search.html` | Mode visual, seleksi, sorotan hasil `/` | F-06 |
| `14-create-move-delete-folder.html` | `<leader>n` dengan peringatan nama, input di explorer dengan error, pindah ke folder, hapus folder (1440×1180) | F-10 |
| `15-sync-conflict-list.html` | Daftar konflik `<leader>c` / `:conflicts` | F-11 |
| `16-sync-conflict-diff.html` | Diff konflik Syncthing | F-11 |
| `17-banners-and-messages.html` | Lembar keadaan: semua banner, variasi status line, pesan baris perintah | F-03, F-04, F-05, F-11 |
| `90-target-final-phase6.html` | Tampilan akhir sekitar Fase 6. **Bukan** spesifikasi Fase 2 | — |

Semua mockup dibuat untuk jendela 1440×860 pada skala 100%, kecuali yang disebut lain. Mockup 14 dan 17 adalah lembar komponen: beberapa keadaan digambar berdampingan, bukan satu layar utuh. Isi catatan di mockup adalah contoh sintetis.

## 3. Warna

Semua pasangan teks dan latar di bawah lolos kontras WCAG AA (minimal 4,5:1 untuk teks biasa). Angka kontras dihitung terhadap latar yang disebut.

### 3.1 Latar dan garis

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `bg.app` | `#121614` | Latar utama, area editor |
| `bg.chrome` | `#0C0F0E` | Bufferline |
| `bg.panel` | `#171C1A` | Explorer, panel samping |
| `bg.statusline` | `#161D1A` | Status line |
| `bg.statusline.segment` | `#1F2925` | Segmen status line yang ditonjolkan (path, `baris:kolom`) |
| `bg.card` | `#1F2624` | Kartu backlink di panel |
| `bg.card.inline` | `#1A201E` | Kartu backlink di bawah catatan, blok kode di mode baca |
| `bg.overlay` | `#172019` | Quick switcher, autocomplete, popup leader, dialog |
| `bg.cursorline` | `#18201D` | Baris tempat kursor berada |
| `bg.selected` | `#22403A` | Item terpilih (explorer, hasil switcher, saran autocomplete) |
| `bg.code.inline` | `#1E2522` | Kode inline di mode baca |
| `bg.scrim` | `#121614` dengan opasitas 65% | Di atas editor selama quick switcher terbuka, sehingga teks di belakangnya tampil 35% (mockup 05) |
| `border.default` | `#242C29` | Garis pemisah panel |
| `border.strong` | `#2E3A35` | Tepi overlay dan dialog |
| `border.control` | `#34403C` | Tepi tombol sekunder |

### 3.2 Teks

| Token | Nilai | Kontras di `bg.app` | Dipakai untuk |
| --- | --- | --- | --- |
| `text.strong` | `#EEF3F1` | 16,3 | Judul, item terpilih |
| `text.primary` | `#D6DEDA` | 13,3 | Teks utama |
| `text.secondary` | `#B5BEBA` | 9,6 | Item explorer, tombol status line tidak aktif |
| `text.muted` | `#9AA4A0` | 7,1 | Keterangan, folder, petunjuk tombol |
| `text.subtle` | `#8D9A94` | 6,2 | Judul bagian huruf kapital (`OUTLINE`, `TAGS`) |
| `text.gutter` | `#7A8882` | 4,9 | Nomor baris, frontmatter di editor |
| `text.on-accent` | `#121614` | 8,6 di `accent` | Teks di badge mode dan tombol utama |

Pada `bg.selected`, keterangan memakai `text.secondary` (5,9:1), bukan `text.muted` yang hanya 4,4:1.

### 3.3 Aksen dan status

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `accent` | `#5CC4A8` | Verdigris: badge NORMAL, heading di editor, sorotan huruf cocok, tombol utama, garis tab aktif |
| `accent.soft` | `#8ADBC5` | Label tag di mode baca |
| `mode.normal` | `#5CC4A8` | Badge NORMAL |
| `mode.insert` | `#E0A46E` | Badge INSERT, kursor garis di insert, titik "belum tersimpan" saat insert |
| `mode.visual` | `#B39DDB` | Badge VISUAL dan pesan `-- VISUAL --` (kontras 7,6:1 dengan `text.on-accent`) |
| `mode.read` | `#7FA6D9` | Badge READ |
| `mode.conflict` | `#E0A46E` | Badge CONFLICT |
| `status.ok` | `#5CC4A8` | Titik "indexed" |
| `status.warning` | `#E0A46E` | Penanda konflik, pesan `W:`, polling watcher |
| `status.error` | `#EF8A80` | Pesan `E:`, `✕ not saved`, tepi input yang ditolak, teks error di bawah input (kontras 7,5:1; 6,9:1 di `bg.overlay`) |
| `status.info` | `#7FA6D9` | Ikon banner info |

Merah hanya dipakai untuk error. Aksi yang memindahkan ke `.trash/` memakai `mode.conflict` (oranye) karena bisa dipulihkan.

### 3.4 Sintaks Markdown di editor (Fase 2)

Karakter tidak disembunyikan, diredupkan, atau diubah ukurannya; hanya diwarnai.

| Elemen | Warna | Gaya |
| --- | --- | --- |
| Heading (`#` sampai teksnya) | `accent` | Tebal |
| Wikilink dan link Markdown | `#D9A47A` (tembaga) | — |
| Kode inline dan blok kode | `#D4C48E` | — |
| Penanda kutipan dan callout (`>`, `[!note]`) | `#9FB8D6` | — |
| Penanda daftar dan checkbox (`-`, `- [ ]`) | `accent` | — |
| Checkbox selesai (`- [x]`) | Teks `text.muted` | — |
| Frontmatter | `text.gutter` | — |
| Tag (`#tag`) | `accent.soft` | — |

### 3.5 Seleksi, pencarian, dan link ambigu di editor

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `editor.selection` | `#263F55` | Latar seleksi mode visual. Semua warna sintaks tetap terbaca di atasnya (≥ 5:1); teks berwarna `text.gutter` di dalam seleksi memakai `text.secondary` |
| `editor.cursor.visual` | `#B39DDB` | Kursor blok di mode visual |
| `editor.search.match` | `#433B1D` | Latar semua hasil `/` yang terlihat |
| `editor.search.current` | `#D4C48E`, teks `#121614` | Hasil `/` yang sedang aktif |
| `editor.link.ambiguous` | Warna link biasa + garis bawah bergelombang `status.warning` | Link yang cocok dengan lebih dari satu catatan |

Status line menampilkan posisi hasil pencarian (`/indeks [2/3]`) dengan warna `#D4C48E`; mode visual menampilkan jumlah baris terpilih.

### 3.6 Mode baca

| Elemen | Warna |
| --- | --- |
| Link ke catatan yang ada | `#D9A47A` |
| Link ke catatan yang belum ada | `#9A8878`, garis bawah titik-titik (kontras 5,4:1) |
| Link yang sedang dipilih dengan `Tab` | Garis tepi 2 px `accent`, jarak 2 px |
| Label tag | Teks `accent.soft` di latar `bg.selected` |

### 3.7 Banner dan diff

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `banner.warning.bg` | `#2A2219` | Latar banner perubahan dari luar |
| `banner.warning.border` | `#4A3A28` | Garis bawah banner, tepi tombol sekunder banner |
| `banner.warning.text` | `#F3E3D2` | Teks utama banner |
| `banner.warning.muted` | `#CBB9A6` | Keterangan banner |
| `diff.disk.bg` | `#3A2A1A` | Baris yang hanya ada di disk |
| `diff.disk.marker` | `#E0A46E` | Nomor baris dan penanda baris disk |
| `diff.mine.bg` | `#1C3340` | Baris yang hanya ada di versi pengguna |
| `diff.mine.marker` | `#7FA6D9` | Nomor baris dan penanda versi pengguna |
| `banner.error.bg` / `.border` / `.text` / `.muted` | `#2E1A1A` / `#5A2E2E` / `#F7DEDB` / `#D9B8B4` | Banner error (simpan gagal) |
| `banner.info.bg` / `.border` / `.text` / `.muted` | `#17222C` / `#2C4258` / `#DCE8F5` / `#B3C3D3` | Banner info (read-only, batas watcher) |

Pada konflik Syncthing, sisi **salinan** memakai warna `diff.disk` (oranye, datang dari luar) dan sisi **asli** memakai `diff.mine` (biru).

Ada tiga jenis banner, semuanya muncul tepat di bawah bufferline dan menggeser editor ke bawah (tidak menutupinya):

| Jenis | Kapan | Contoh |
| --- | --- | --- |
| Peringatan | Data bisa hilang bila pengguna salah pilih | Perubahan dari luar, file dihapus dari luar, konflik Syncthing |
| Error | Sesuatu gagal dan Patinote sedang mencoba lagi | Simpan gagal |
| Info | Keterbatasan yang perlu diketahui, tanpa risiko data | Read-only, batas watcher |

Diff memakai oranye dan biru, bukan merah dan hijau, agar terbaca oleh pengguna buta warna merah-hijau. Warna tidak pernah menjadi satu-satunya penanda: setiap sisi diff juga diberi judul ("On disk", "Your version") dan keterangan.

## 4. Huruf

Font dibawa bersama aplikasi (lisensi SIL OFL, kompatibel MIT untuk distribusi): **Inter 4.1** (Regular, Italic, Medium, SemiBold, Bold) dan **JetBrains Mono 2.304** (Regular, SemiBold, Bold). Filenya ada di `crates/patinote-app/assets/fonts/` dan dimuat dari byte saat aplikasi mulai; font sistem tidak dipindai. Mockup memuat file yang sama lewat `docs/mockups/fonts.css`.

| Token | Font | Ukuran / tinggi baris | Ketebalan | Dipakai untuk |
| --- | --- | --- | --- | --- |
| `type.editor` | JetBrains Mono | 15 px / 26 px | 400, heading 700 | Teks di editor |
| `type.focus` | JetBrains Mono | 16 px / 30 px | 400 | Editor di mode focus |
| `type.mono.ui` | JetBrains Mono | 13 px | 400, badge 700 | Bufferline, status line, baris perintah, popup leader, petunjuk tombol |
| `type.ui` | Inter | 14 px / 1,4 | 400, 500, 600 | Teks antarmuka umum |
| `type.ui.small` | Inter | 13 px / 1,5 | 400 | Explorer, cuplikan backlink |
| `type.label` | Inter | 11–12 px, huruf kapital, jarak huruf 0,8 px | 600 | Judul bagian (`OUTLINE`, `TAGS`) |
| `type.read.body` | Inter | 16 px / 1,7 | 400 | Isi mode baca |
| `type.read.h1` | Inter | 32 px / 1,25 | 700 | Heading 1 di mode baca |
| `type.read.h2` | Inter | 24 px / 1,3 | 700 | Heading 2 di mode baca |
| `type.dialog.title` | Inter | 17 px | 600 | Judul dialog |
| `type.title` | Inter | 22 px / 1,3 | 700 | Judul halaman: kartu layar awal, nama vault di layar tanpa catatan terbuka |

Semua ukuran adalah piksel logis pada skala 100% dan ikut skala HiDPI dari GNOME. `Ctrl+=`, `Ctrl+-`, dan `Ctrl+0` mengubah semua ukuran huruf secara proporsional (langkah 10%, batas 70–200%).

## 5. Ukuran dan jarak

**Skala jarak:** 2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 32, 48 px. Pakai nilai dari skala ini saja.

**Sudut:** 3 px (baris kursor), 5–6 px (item daftar, tombol, kartu), 8 px (popup autocomplete dan leader, banner), 10 px (quick switcher dan dialog), penuh (label tag).

| Token | Nilai | Elemen |
| --- | --- | --- |
| `size.bufferline.h` | 32 px | Bufferline mode minimal |
| `size.tabbar.h` | 40 px | Bar tab mode classic |
| `size.statusline.h` | 30 px | Status line |
| `size.cmdline.h` | 26 px | Baris perintah |
| `size.explorer.w` | 240 px default, 180–480 px bila diseret | Explorer |
| `size.sidepanel.w` | 300 px default, 240–480 px bila diseret | Panel samping |
| `size.editor.maxw` | 920 px (sekitar 100 karakter) | Kolom editor, mode minimal |
| `size.editor.maxw.classic` | 820 px | Kolom editor, mode classic |
| `size.editor.maxw.focus` | 760 px | Kolom editor, mode focus |
| `size.read.maxw` | 720 px | Kolom mode baca |
| `size.gutter.w` | 44 px | Kolom nomor baris |
| `size.switcher.w` | 640 px, 72 px dari atas | Quick switcher |
| `size.autocomplete.w` | 460 px, tepat di bawah baris kursor | Saran `[[` |
| `size.leader.w` | 600 px, 64 px di atas status line, di tengah | Popup leader |
| `size.dialog.w` | 460–520 px | Dialog ganti nama dan hapus |
| `size.picker.w` | 460 px | Pilihan link ambigu (tepat di bawah baris kursor), pindah ke folder |
| `size.start.maxw` | 560 px | Kolom layar tanpa catatan terbuka |
| `size.welcome.w` | 480 px | Kartu layar awal |
| `size.control.h` | 36–40 px | Tombol dan kolom input |
| `size.list.row` | 34–40 px | Baris di explorer, switcher, autocomplete |
| `size.icon` | 18 px | Ikon garis di banner (`crates/patinote-app/assets/icons/`) |
| `size.logo` / `size.logo.large` | 16 px / 28 px | Tanda Patinote di bufferline / di kartu layar awal |
| `size.shortcut.key.w` | 64 px | Kolom tombol di daftar pintasan layar tanpa catatan terbuka |
| `size.recent.time.w` | 84 px | Kolom waktu di daftar catatan terakhir |
| `size.cursor.insert.w` | 2 px | Kursor garis di mode insert |
| `size.editor.cell.w` | 9 px | Lebar satu sel JetBrains Mono 15 px; kursor blok di baris kosong |
| `size.wave.step` / `size.wave.offset` | 2 px / 4 px | Garis bawah bergelombang link ambigu: langkah zigzag, dan jaraknya dari dasar teks |

Jarak dalam yang sering dipakai: area editor 28 px atas dan 32 px samping; panel 16 px; overlay 14–16 px; dialog 22–24 px.

## 6. Komponen

| Komponen | Keadaan yang harus ada | Catatan |
| --- | --- | --- |
| Badge mode | NORMAL, INSERT, VISUAL, READ, CONFLICT | Teks `text.on-accent` tebal di warna `mode.*` |
| Tab buffer | Aktif, tidak aktif, belum tersimpan (titik `●`) | Aktif: latar `#1A211E` dan garis bawah 2 px `accent` (classic: garis atas) |
| Item daftar | Normal, terpilih, sorotan huruf cocok | Huruf cocok tebal berwarna `accent` |
| Kartu backlink | Normal, fokus keyboard | Nama catatan tebal, cuplikan dengan link berwarna tembaga, penanda `→ #heading` bila ke heading |
| Tombol | Utama, sekunder, tombol di status line (aktif dan tidak) | Setiap tombol menampilkan pintasan keyboard-nya bila ada |
| Banner | Peringatan, error, info | Peringatan dan error memakai role `alert`, info memakai `status`; tombol aksi utama paling kanan |
| Pesan baris perintah | Info, peringatan (`W:`), error (`E:`) | Info `text.muted`, peringatan `status.warning`, error `status.error`. Pesan hilang pada tombol berikutnya; error bertahan minimal 5 detik |
| Input | Normal, peringatan, ditolak | Tepi `accent` saat fokus, `status.warning` bila nama berisiko (`#`, `^`, `[`, `]`), `status.error` bila ditolak; alasan ditulis di bawah input, bukan hanya warna |
| Pemilih daftar | Switcher, autocomplete, link ambigu, pindah folder, daftar konflik | Satu pola: kotak `bg.overlay`, item terpilih `bg.selected`, huruf cocok `accent`, baris petunjuk tombol di bawah |
| Popup leader | Muncul 300 ms setelah Spasi | Grid empat kolom, huruf pintasan berwarna `accent` |
| Kursor | Blok (normal), garis 2 px (insert) | Kursor insert berwarna `mode.insert` |

## 7. Keadaan yang tidak digambar

Keadaan berikut mengikuti pola yang sudah ada di mockup:

- **Quick switcher, kotak kosong:** daftar catatan terakhir dibuka, sama dengan mockup 05 tanpa sorotan huruf. Di vault yang belum pernah dipakai: satu baris `text.muted` "No recent notes yet".
- **Quick switcher, tanpa hasil:** satu baris `text.muted` `No notes match "xyz"`, dengan petunjuk `Shift+Enter create "xyz"` tetap di baris bawah.
- **Autocomplete, tanpa kecocokan:** satu baris `Enter inserts [[xyz]] as a link to a new note`.
- **Autocomplete heading (`[[Catatan#`):** kotak yang sama dengan mockup 06. Setiap baris berisi teks heading, diawali `#` sampai `######` berwarna `text.muted`, dan menjorok 12 px per tingkat.
- **Layar tanpa catatan terbuka di vault baru:** bagian "Recent notes" diganti satu baris "No recent notes yet. Press ␣f to find a note or ␣n to create one."
- **Explorer di vault kosong:** satu baris `text.muted` "This vault is empty. Press a to create a note."
- **Catatan dimuat ulang dari luar:** pesan info di baris perintah, `"Nama.md" updated from disk`.
- **Fokus keyboard di panel samping:** item yang difokus memakai `bg.selected`; bila fokus ada di editor, item di panel tidak disorot.
- **Panel diseret:** batas panel menyorot `accent` selebar 2 px saat kursor mouse di atasnya.

## 8. Aksesibilitas

- Kontras minimal 4,5:1 untuk semua teks; token di atas sudah memenuhi.
- Warna tidak pernah menjadi satu-satunya penanda (mode juga ditulis sebagai teks, diff diberi judul).
- Semua elemen bisa dicapai dengan keyboard; fokus keyboard selalu terlihat: `bg.selected` untuk item daftar, garis tepi `accent` 2 px untuk tombol dan input.
- Gerakan dan animasi: tidak ada animasi selain munculnya popup; tidak ada yang berkedip.

## 9. Perbedaan dengan mockup

Per 20 November 2026 semua mockup sudah memakai nilai di dokumen ini (nomor baris `#7A8882`, link ke catatan yang belum ada `#9A8878`, popup leader dengan `l`). Bila nanti mockup dan dokumen ini berbeda, catat perbedaannya di sini; dokumen ini tetap yang benar.

Ditemukan di langkah 2.1 (mockup 17). Aplikasi memakai nilai dokumen ini:

| Elemen di mockup 17 | Mockup | Dipakai aplikasi |
| --- | --- | --- |
| Tepi tombol sekunder banner peringatan | `#5A4A38` | `banner.warning.border` (bagian 3.7 menyebutnya untuk tepi tombol sekunder banner) |
| Tepi tombol sekunder banner info | `#34506A` | `banner.info.border` |
| Teks tombol utama banner peringatan dan error | `#1A140D`, `#1F0F0E` | `text.on-accent` (kontras 8,4:1 dan 7,5:1) |
| Latar kotak perintah di banner info | `#0F1412`, sudut 4 px | `bg.chrome`, sudut 3 px |
| Kalimat pertama banner | tebal 700 | 600, batas `type.ui` |
| Tinggi baris teks banner | 1,5 | 1,4 (`type.ui`) |
| Jarak huruf `type.label` | 0,8 px | Belum diterapkan: Iced 0.14 tidak bisa mengatur jarak huruf |
| Teks keterangan di bawah contoh | 12,5 px / 1,5 | `type.ui.small` (13 px / 1,5) |
| Latar lembar komponen | `#0E1210` | `bg.chrome`. Hanya lembar, bukan bagian aplikasi |

Ditemukan di langkah 2.2 (mockup 01, 10, 11):

| Elemen | Mockup | Dipakai aplikasi |
| --- | --- | --- |
| Judul kartu "Vault not found" | 20 px | `type.title` (22 px), sama dengan kartu sambutan |
| Teks paragraf kartu layar awal | 14 px / 1,6 | `type.ui` (14 px / 1,4) |
| Jarak dalam kartu layar awal | 28 px | `PAD_DIALOG` (24 px); 28 px tidak ada di skala jarak |
| Teks tombol utama di kartu layar awal | `#0F1412` | `text.on-accent` |
| Warna "Patinote" di bufferline | `#E6EEEA` | `text.strong` |
| Folder dan waktu di daftar catatan terakhir | 12 px | `type.ui.small` (13 px) |
| Petunjuk `j/k move · Enter open` | 12 px | `type.mono.ui` (13 px) |
| Jarak atas layar awal dan layar tanpa catatan terbuka | 96 px / 72 px | 96 px (2 × 48) / 72 px (48 + 24) dari skala jarak |


## 10. Cadangan bila editor tidak mendukung

Langkah 0.6 menguji apakah editor Iced bisa menggambar elemen di bawah ini (daftar lengkap di `CLAUDE.md`, bagian 3). Bila sebuah elemen tidak bisa dibuat dengan wajar, pakai cadangannya, catat di `0002`, dan beri tahu pemilik sebelum Fase 2 dimulai. Cadangan tidak boleh menghilangkan informasi; hanya bentuk penandanya yang berubah.

| Elemen | Desain | Cadangan |
| --- | --- | --- |
| Seleksi mode visual | Latar `editor.selection` per bagian teks | Wajib ada. Bila widget bawaan tidak bisa, pakai widget editor sendiri; tidak ada cadangan visual |
| Semua hasil `/` | Latar `editor.search.match` | Teks hasil diberi warna `#D4C48E` tebal tanpa latar; status line tetap menampilkan `[2/3]` |
| Hasil `/` yang aktif | Latar `editor.search.current`, teks gelap | Kursor ditaruh di awal hasil; status line menampilkan posisinya |
| Baris kursor | Latar `bg.cursorline` sebaris penuh | Hanya nomor baris aktif yang ditebalkan dan diterangkan (sudah ada di desain) |
| Link ambigu | Garis bawah bergelombang `status.warning` | Garis bawah lurus `status.warning`; bila garis bawah tidak bisa sama sekali, teks link berwarna `status.warning` |
| Kursor | Blok di normal dan visual, garis 2 px di insert | Wajib ada; bentuk kursor membedakan mode bersama badge |
| Nomor baris relatif | Kolom terpisah, sejajar dengan soft wrap | Wajib ada; bila tidak bisa sejajar saat soft wrap, baris lanjutan tidak bernomor |

