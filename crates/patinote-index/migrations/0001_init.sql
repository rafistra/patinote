-- Patinote: skema indeks awal (migrasi 0001)
-- Lokasi DB: $XDG_CACHE_HOME/patinote/<vault-id>/index.db
-- Indeks hanyalah cache: aman dihapus, selalu bisa dibangun ulang dari vault.
-- Pragma diatur saat koneksi dibuka (bukan di migrasi):
--   PRAGMA journal_mode = WAL;
--   PRAGMA synchronous = NORMAL;
--   PRAGMA foreign_keys = ON;
-- Versi skema disimpan di PRAGMA user_version (dikelola runner migrasi).

-- Metadata indeks: versi aplikasi, path vault, waktu indeks penuh terakhir.
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- Satu baris per file yang dikenali di vault: catatan .md, lampiran, canvas.
-- Lampiran ikut dicatat karena wikilink bisa menunjuk ke gambar atau PDF.
CREATE TABLE notes (
    id          INTEGER PRIMARY KEY,
    path        TEXT    NOT NULL UNIQUE,   -- relatif ke root vault, separator '/', Unicode NFC
    path_lower  TEXT    NOT NULL,          -- untuk resolusi link case-insensitive
    stem        TEXT    NOT NULL,          -- nama file tanpa ekstensi
    stem_lower  TEXT    NOT NULL,
    ext         TEXT    NOT NULL,          -- 'md', 'png', 'pdf', 'canvas', ...
    kind        TEXT    NOT NULL CHECK (kind IN ('note', 'attachment', 'canvas')),
    title       TEXT,                      -- frontmatter title > H1 pertama > stem
    size        INTEGER NOT NULL,
    mtime_ns    INTEGER NOT NULL,          -- jalur cepat deteksi perubahan
    hash        BLOB,                      -- blake3 32 byte; NULL untuk lampiran besar
    frontmatter TEXT,                      -- frontmatter mentah sebagai JSON (fidelitas penuh)
    indexed_at  INTEGER NOT NULL           -- unix epoch detik
) STRICT;

CREATE INDEX notes_stem_lower ON notes (stem_lower);
CREATE INDEX notes_path_lower ON notes (path_lower);

-- Alias dari frontmatter `aliases:`; dipakai resolusi link dan autocomplete.
CREATE TABLE aliases (
    note_id     INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    alias       TEXT    NOT NULL,
    alias_lower TEXT    NOT NULL,
    PRIMARY KEY (note_id, alias_lower)
) STRICT, WITHOUT ROWID;

CREATE INDEX aliases_lower ON aliases (alias_lower);

-- Semua link keluar. dst_id NULL = link ke catatan yang belum ada (unresolved).
-- Posisi byte disimpan agar rename bisa menulis ulang link secara presisi.
CREATE TABLE links (
    id           INTEGER PRIMARY KEY,
    src_id       INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    dst_id       INTEGER REFERENCES notes (id) ON DELETE SET NULL,
    kind         TEXT    NOT NULL CHECK (kind IN ('wiki', 'embed', 'markdown', 'frontmatter')),
    raw          TEXT    NOT NULL,         -- teks asli, mis. '[[Rapat IKN#Agenda|rapat]]'
    target       TEXT    NOT NULL,         -- bagian path, mis. 'Rapat IKN'
    target_lower TEXT    NOT NULL,         -- untuk me-resolve ulang saat catatan baru dibuat
    subpath      TEXT,                     -- 'Agenda' (heading) atau '^abc123' (blok)
    display      TEXT,                     -- teks alias setelah '|'
    line         INTEGER NOT NULL,         -- baris (mulai 1), untuk konteks dan lompat
    byte_start   INTEGER NOT NULL,
    byte_end     INTEGER NOT NULL
) STRICT;

CREATE INDEX links_src ON links (src_id);
CREATE INDEX links_dst ON links (dst_id);
CREATE INDEX links_unresolved ON links (target_lower) WHERE dst_id IS NULL;

-- Tag inline (#tag) dan dari frontmatter. Tag bersarang disimpan utuh: 'proyek/patinote'.
-- Query hierarki: tag = 'proyek' OR tag LIKE 'proyek/%'.
CREATE TABLE tags (
    note_id INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    tag     TEXT    NOT NULL,              -- dinormalisasi: tanpa '#', huruf kecil
    display TEXT    NOT NULL,              -- ejaan asli untuk ditampilkan
    source  TEXT    NOT NULL CHECK (source IN ('inline', 'frontmatter')),
    line    INTEGER                        -- NULL untuk tag frontmatter
) STRICT;

CREATE INDEX tags_tag ON tags (tag);
CREATE INDEX tags_note ON tags (note_id);

-- Heading: outline, link [[catatan#bagian]], dan rename heading kelak.
CREATE TABLE headings (
    note_id    INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    level      INTEGER NOT NULL CHECK (level BETWEEN 1 AND 6),
    text       TEXT    NOT NULL,
    slug       TEXT    NOT NULL,           -- bentuk ternormalisasi untuk mencocokkan subpath
    line       INTEGER NOT NULL,
    byte_start INTEGER NOT NULL
) STRICT;

CREATE INDEX headings_note ON headings (note_id, line);

-- ID blok '^abc123' untuk link [[catatan#^abc123]].
CREATE TABLE blocks (
    note_id  INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    block_id TEXT    NOT NULL,
    line     INTEGER NOT NULL,
    PRIMARY KEY (note_id, block_id)
) STRICT, WITHOUT ROWID;

-- Properti frontmatter, satu baris per nilai skalar. Daftar dipecah per item (idx),
-- supaya query ala Dataview (Fase 4) bisa memakai indeks biasa.
CREATE TABLE properties (
    note_id    INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    key        TEXT    NOT NULL,           -- ejaan asli
    key_lower  TEXT    NOT NULL,
    idx        INTEGER NOT NULL DEFAULT 0, -- posisi dalam daftar; 0 untuk skalar
    type       TEXT    NOT NULL CHECK (type IN ('text', 'number', 'bool', 'date', 'datetime', 'null')),
    value_text TEXT,                       -- teks, tanggal ISO 8601, 'true'/'false'
    value_num  REAL,                       -- angka; juga epoch untuk date/datetime
    PRIMARY KEY (note_id, key_lower, idx)
) STRICT, WITHOUT ROWID;

CREATE INDEX properties_key_text ON properties (key_lower, value_text);
CREATE INDEX properties_key_num  ON properties (key_lower, value_num);

-- Peringatan per file yang tidak menghentikan pengindeksan (Q-005): file bukan UTF-8,
-- catatan di atas 5 MB, file yang tidak bisa dibaca, frontmatter rusak.
CREATE TABLE warnings (
    note_id INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    code    TEXT    NOT NULL CHECK (code IN ('not_utf8', 'too_large', 'unreadable', 'frontmatter_invalid')),
    line    INTEGER                        -- NULL bila tidak terkait baris tertentu
) STRICT;

CREATE INDEX warnings_note ON warnings (note_id);

-- Pencarian full-text. rowid = notes.id. Menyimpan salinan isi agar snippet() dan
-- highlight() bekerja tanpa membaca file. remove_diacritics 2 menyamakan 'é' dan 'e'.
CREATE VIRTUAL TABLE notes_fts USING fts5 (
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);
