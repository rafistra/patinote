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
