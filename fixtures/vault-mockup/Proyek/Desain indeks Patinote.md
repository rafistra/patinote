---
tags: [proyek/patinote, rust]
status: draft
project: "[[Patinote]]"
---
# Desain indeks Patinote

Hari ini fokus ke skema SQLite untuk [[Patinote]]. Keputusan dari [[Rapat 19 Nov]] dan ide lanjutan di [[Ide Baru]]:
- Indeks disimpan di `~/.cache/patinote/`, bukan di vault
- Mulai dengan FTS5, pindah ke [[tantivy]] kalau benchmark gagal

## Tabel inti
- [ ] Finalisasi tabel `links` dengan posisi byte
- [x] Uji query backlinks di SQLite

| Tabel | Isi |
| --- | --- |
| `notes` | Satu baris per file |
| `links` | Semua link keluar beserta posisi byte |

```sql
SELECT src_id FROM links WHERE dst_id = ?;
```

![[diagram-indeks.png]]

![[Patinote]]

> [!note] Indeks hanyalah cache
> Aman dihapus kapan saja dan bisa dibangun ulang dari vault.

Kompleksitas pencarian: $O(\log n)$ per query.

> Kutipan biasa: indeks harus bisa dibangun ulang tanpa kehilangan apa pun.

---

Teks **tebal**, *miring*, tag #rust, dan [dokumentasi SQLite](https://sqlite.org/fts5.html).
