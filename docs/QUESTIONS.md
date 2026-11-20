# Pertanyaan terbuka

Agent mencatat di sini setiap pertanyaan yang membuatnya berhenti (lihat `CLAUDE.md`, bagian 11). Pemilik menjawab di bawah pertanyaannya; jawaban yang mengubah kebutuhan dipindahkan ke `docs/PRD.md` atau dokumen desain.

Format:

```markdown
## Q-001 · Judul singkat (langkah 1.3, 20 November 2026)

**Konteks:** apa yang sedang dikerjakan dan apa yang tidak jelas.
**Pilihan:** A ..., B ...; rekomendasi agent dan alasannya.
**Jawaban:** (diisi pemilik)
```

## Q-001 · Isi "pemeriksaan data vault" di CI dan pre-commit (langkah 0.4, 20 November 2026)

**Konteks:** `CLAUDE.md` langkah 0.4 meminta pemeriksaan data vault, dan PRD (Risiko) menyebut pemeriksaan pre-commit agar isi vault pribadi tidak masuk repo. Isi pemeriksaannya tidak dirinci. CI tidak boleh membaca vault asli, jadi CI hanya bisa memeriksa aturan umum; pencocokan dengan vault asli hanya mungkin di laptop pemilik.
**Pilihan:**
- A. Lokasi file: file berbentuk vault (`.md`, `.canvas`, `.obsidian/`, `*.sync-conflict-*`, `.trash/`) hanya boleh ada di folder yang diizinkan (`docs/`, `fixtures/`, korpus uji, `README.md`, `CLAUDE.md`). Jalan di CI dan pre-commit.
- B. Fixture dibekukan: `fixtures/vault-mockup/` dijaga dengan daftar hash; mengubahnya harus memperbarui daftar itu dengan sengaja. Jalan di CI dan pre-commit.
- C. Path pribadi: menolak path absolut `/home/<nama>/` di file yang di-commit. Jalan di CI dan pre-commit.
- D. Pencocokan dengan vault asli, hanya di pre-commit dan hanya bila `PATINOTE_REAL_VAULT` diset: menolak commit bila file yang di-stage identik dengan file di vault asli, atau baris yang ditambahkan memuat nama catatan vault asli. Hanya mencetak path di repo dan jumlah kecocokan, tidak pernah nama atau isi catatan.

Rekomendasi agent: A, B, C, dan D. A sampai C murah dan menutup kesalahan yang umum; D satu-satunya yang benar-benar mengenali data pribadi, dan karena hanya berjalan di laptop pemilik, aturan CI di `CLAUDE.md` bagian 7 tetap terpenuhi.
**Jawaban:** A, B, C, dan D disetujui pemilik (20 November 2026), termasuk izin bagi hook pre-commit untuk membaca nama file dan hash vault asli bila `PATINOTE_REAL_VAULT` diset.

## Q-002 · Bentuk JSON korpus uji emas parser (langkah 0.5, 20 November 2026)

**Konteks:** F-02.1 meminta sekitar 40 file `.md`, masing-masing dengan `.json` hasil parse yang diharapkan. Tipe hasil parser baru dibuat di langkah 1.1, jadi bentuk JSON-nya harus ditetapkan sekarang dan nanti diikuti parser.
**Pilihan:**
- A. Tetapkan sekarang, mengikuti kolom skema `0001_init.sql`. Isinya: `title`; `aliases`; `links` (`kind`, `raw`, `target`, `subpath`, `display`, `line`, `byte_start`, `byte_end`); `tags` (`tag`, `display`, `source`, `line`); `headings` (`level`, `text`, `line`, `byte_start`); `blocks` (`block_id`, `line`); `properties` (`key`, `idx`, `type`, `value`); `warnings`. Aturannya: posisi byte dihitung dari byte file apa adanya (termasuk BOM dan frontmatter), nomor baris mulai dari 1 dihitung dari awal file, `slug` heading tidak ikut karena urusan resolver (1.4). Keputusan ini dicatat di `docs/design/0004-korpus-uji-parser.md`.
- B. Langkah 0.5 hanya membuat file `.md`; `.json` ditulis di langkah 1.1 bersama parsernya.

Rekomendasi agent: A. Bentuknya hampir seluruhnya mengikuti skema yang sudah diterima, dan korpus yang dibuat sebelum parser menjadi patokan yang independen, bukan sekadar menyalin hasil parser.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-003 · Lokasi benchmark (langkah 0.5, 20 November 2026)

**Konteks:** `0001` bagian 1 menggambar folder `benches/` di root repo, sedangkan `CLAUDE.md` bagian 8 menjalankan `cargo bench -p patinote-index`. Root repo adalah workspace tanpa paket, jadi tidak bisa punya `benches/` sendiri.
**Pilihan:**
- A. Benchmark di `crates/patinote-index/benches/` (dan nanti di crate lain bila perlu); gambar di `0001` bagian 1 diperbaiki.
- B. Membuat crate khusus `benches/` di root sebagai anggota workspace.

Rekomendasi agent: A, karena cocok dengan perintah di `CLAUDE.md` dan tidak menambah crate.
**Jawaban:** A, disetujui pemilik (20 November 2026).
