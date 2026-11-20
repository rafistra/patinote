//! Generates deterministic synthetic vaults for Patinote tests and benchmarks.
//!
//! The same [`Config`] always produces byte-identical vaults. Every name and
//! sentence is synthetic (CLAUDE.md, section 7). The returned [`Summary`]
//! counts what was written, so index tests can compare against it.

mod rng;
mod words;

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use rng::Rng;
use words::{FOLDERS, STATUSES, TAGS, UNRESOLVED_PREFIX, WORDS};

/// What to generate.
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of notes (`.md` files, not counting Syncthing conflict copies).
    pub notes: usize,
    /// Seed for the generator. The same seed gives the same vault.
    pub seed: u64,
    /// When non-zero, the first note is padded to at least this many lines.
    pub long_note_lines: usize,
}

/// Counts of what [`generate`] wrote.
///
/// Links are counted the way Obsidian reads them: links inside code blocks and
/// inline code are not links, and Syncthing conflict copies are not notes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub notes: usize,
    pub attachments: usize,
    pub sync_conflicts: usize,
    pub folders: usize,
    pub links_wiki: usize,
    pub links_embed: usize,
    pub links_markdown: usize,
    pub links_frontmatter: usize,
    /// Links whose target does not exist (a subset of all links).
    pub links_unresolved: usize,
    /// Name-only links to a name shared by two notes (a subset of all links).
    pub links_ambiguous: usize,
    /// Note names used by two notes in different folders.
    pub ambiguous_names: usize,
    pub aliases: usize,
    pub inline_tags: usize,
    pub frontmatter_tags: usize,
    pub headings: usize,
    pub blocks: usize,
    pub crlf_notes: usize,
    pub bom_notes: usize,
    /// Lines in the long note, or 0 when none was requested.
    pub long_note_lines: usize,
}

impl Summary {
    /// Total number of links of every kind.
    pub fn links(&self) -> usize {
        self.links_wiki + self.links_embed + self.links_markdown + self.links_frontmatter
    }

    /// Serializes the summary as a single-line JSON object.
    pub fn to_json(&self) -> String {
        let fields = [
            ("notes", self.notes),
            ("attachments", self.attachments),
            ("sync_conflicts", self.sync_conflicts),
            ("folders", self.folders),
            ("links", self.links()),
            ("links_wiki", self.links_wiki),
            ("links_embed", self.links_embed),
            ("links_markdown", self.links_markdown),
            ("links_frontmatter", self.links_frontmatter),
            ("links_unresolved", self.links_unresolved),
            ("links_ambiguous", self.links_ambiguous),
            ("ambiguous_names", self.ambiguous_names),
            ("aliases", self.aliases),
            ("inline_tags", self.inline_tags),
            ("frontmatter_tags", self.frontmatter_tags),
            ("headings", self.headings),
            ("blocks", self.blocks),
            ("crlf_notes", self.crlf_notes),
            ("bom_notes", self.bom_notes),
            ("long_note_lines", self.long_note_lines),
        ];
        let body: Vec<String> = fields.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!("{{\"v\":1,{}}}", body.join(","))
    }
}

/// One planned note.
struct Meta {
    folder: usize,
    stem: String,
    alias: Option<String>,
}

/// Generates a vault into `out`, which must not exist or be an empty folder.
pub fn generate(config: &Config, out: &Path) -> io::Result<Summary> {
    if out.exists() && fs::read_dir(out)?.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is not empty", out.display()),
        ));
    }
    fs::create_dir_all(out)?;

    let mut rng = Rng::new(config.seed);
    let mut summary = Summary::default();

    let folders = folder_list(config.notes);
    let metas = plan_notes(config, &folders, &mut rng, &mut summary);
    let ambiguous: HashSet<String> = ambiguous_stems(&metas);

    // Attachments: tiny identical PNG files.
    let attachments: Vec<String> = (0..config.notes / 50)
        .map(|k| format!("gambar-{k:04}.png"))
        .collect();
    if !attachments.is_empty() {
        fs::create_dir_all(out.join("Lampiran"))?;
    }
    for name in &attachments {
        fs::write(out.join("Lampiran").join(name), PNG_1X1)?;
    }
    summary.attachments = attachments.len();

    // Obsidian-style config folder, ignored by Patinote.
    fs::create_dir_all(out.join(".obsidian"))?;
    fs::write(out.join(".obsidian/app.json"), "{}\n")?;

    // Notes that get a Syncthing conflict copy, spread evenly.
    let conflicts: HashSet<usize> = conflict_indices(config.notes);

    let mut used_folders = HashSet::new();
    let ctx = Ctx {
        metas: &metas,
        folders: &folders,
        ambiguous: &ambiguous,
        attachments: &attachments,
    };
    for (i, meta) in metas.iter().enumerate() {
        let mut text = ctx.note_text(i, config, &mut rng, &mut summary);
        if i % 20 == 7 {
            text = text.replace('\n', "\r\n");
            summary.crlf_notes += 1;
        }
        if i % 100 == 13 {
            text.insert(0, '\u{feff}');
            summary.bom_notes += 1;
        }

        let folder = folders[meta.folder].as_str();
        used_folders.insert(folder);
        let dir = out.join(folder);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{}.md", meta.stem)), &text)?;

        if conflicts.contains(&i) {
            let k = summary.sync_conflicts;
            let name = format!(
                "{}.sync-conflict-20261119-{:06}-SYNTHET.md",
                meta.stem,
                120_000 + k
            );
            fs::write(
                dir.join(name),
                format!("{text}\nEdited on another device.\n"),
            )?;
            summary.sync_conflicts += 1;
        }
    }
    summary.notes = metas.len();
    summary.folders = used_folders.iter().filter(|f| !f.is_empty()).count();
    Ok(summary)
}

/// A 1×1 transparent PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// Folders for a vault of `notes` notes: the fixed list plus one project
/// folder per 250 notes.
fn folder_list(notes: usize) -> Vec<String> {
    let mut folders: Vec<String> = FOLDERS.iter().map(|f| (*f).to_owned()).collect();
    for k in 1..=notes / 250 {
        folders.push(format!("Proyek/Topik {k:03}"));
    }
    folders
}

fn plan_notes(
    config: &Config,
    folders: &[String],
    rng: &mut Rng,
    summary: &mut Summary,
) -> Vec<Meta> {
    let width = config.notes.max(1).to_string().len().max(4);
    // Index of `Harian` in FOLDERS; checked by a test.
    let harian = 1;
    let mut metas: Vec<Meta> = Vec::with_capacity(config.notes);
    let mut day = 0;
    for i in 0..config.notes {
        let meta = if i == 0 && config.long_note_lines > 0 {
            Meta {
                folder: 0,
                stem: "Catatan panjang".to_owned(),
                alias: None,
            }
        } else if i % 10 == 1 {
            day += 1;
            Meta {
                folder: harian,
                stem: date_from_days(DAY_2024_01_01 + day),
                alias: None,
            }
        } else if i % 40 == 39 {
            // Same name as the previous note, in another folder: an ambiguous name.
            let prev = &metas[i - 1];
            summary.ambiguous_names += 1;
            Meta {
                folder: (prev.folder + 1) % folders.len(),
                stem: prev.stem.clone(),
                alias: None,
            }
        } else {
            let stem = format!(
                "{} {} {:0width$}",
                capitalize(rng.pick(WORDS)),
                rng.pick(WORDS),
                i
            );
            let alias = rng.chance(10).then(|| {
                format!(
                    "{} {} alias {i}",
                    capitalize(rng.pick(WORDS)),
                    rng.pick(WORDS)
                )
            });
            if alias.is_some() {
                summary.aliases += 1;
            }
            Meta {
                folder: rng.below(folders.len()),
                stem,
                alias,
            }
        };
        metas.push(meta);
    }
    metas
}

fn ambiguous_stems(metas: &[Meta]) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut dup = HashSet::new();
    for m in metas {
        let key = m.stem.to_lowercase();
        if !seen.insert(key.clone()) {
            dup.insert(key);
        }
    }
    dup
}

fn conflict_indices(notes: usize) -> HashSet<usize> {
    let count = notes / 500;
    if count == 0 {
        return HashSet::new();
    }
    let step = notes / count;
    (0..count)
        .map(|k| 2 + k * step)
        .filter(|&i| i < notes)
        .collect()
}

struct Ctx<'a> {
    metas: &'a [Meta],
    folders: &'a [String],
    ambiguous: &'a HashSet<String>,
    attachments: &'a [String],
}

impl Ctx<'_> {
    fn folder_of(&self, j: usize) -> &str {
        self.folders[self.metas[j].folder].as_str()
    }

    fn is_ambiguous(&self, j: usize) -> bool {
        self.ambiguous.contains(&self.metas[j].stem.to_lowercase())
    }

    /// Writes one link to a random target and updates the counts.
    fn link(&self, rng: &mut Rng, summary: &mut Summary) -> String {
        let j = rng.below(self.metas.len());
        let target = &self.metas[j];
        let stem = target.stem.as_str();
        let name_only_ambiguous = self.is_ambiguous(j);
        let form = rng.below(11);
        let (text, by_name) = match form {
            0 => (format!("[[{stem}]]"), true),
            1 => (
                format!("[[{stem}|{} {}]]", rng.pick(WORDS), rng.pick(WORDS)),
                true,
            ),
            2 => {
                let sub = if rng.chance(50) {
                    "Ringkasan"
                } else {
                    "Detail#Tindak lanjut"
                };
                (format!("[[{stem}#{sub}]]"), true)
            }
            3 => (format!("[[{stem}#^b{j}]]"), true),
            4 => {
                let folder = self.folder_of(j);
                if folder.is_empty() {
                    (format!("[[{stem}]]"), true)
                } else {
                    (format!("[[{folder}/{stem}]]"), false)
                }
            }
            5 => (format!("[[{stem}.md]]"), true),
            6 => {
                summary.links_embed += 1;
                let text = format!("![[{stem}]]");
                if name_only_ambiguous {
                    summary.links_ambiguous += 1;
                }
                return text;
            }
            7 => {
                summary.links_embed += 1;
                return match self.attachments.get(rng.below(self.attachments.len())) {
                    Some(name) if rng.chance(50) => format!("![[{name}|300]]"),
                    Some(name) => format!("![[{name}]]"),
                    None => {
                        if name_only_ambiguous {
                            summary.links_ambiguous += 1;
                        }
                        format!("![[{stem}]]")
                    }
                };
            }
            8 => {
                summary.links_markdown += 1;
                let folder = self.folder_of(j);
                let path = if folder.is_empty() {
                    format!("{stem}.md")
                } else {
                    format!("{folder}/{stem}.md")
                };
                return format!("[{}]({})", rng.pick(WORDS), path.replace(' ', "%20"));
            }
            9 => match &target.alias {
                Some(alias) => (format!("[[{alias}]]"), false),
                None => (format!("[[{stem}]]"), true),
            },
            _ => {
                summary.links_unresolved += 1;
                summary.links_wiki += 1;
                return format!(
                    "[[{UNRESOLVED_PREFIX} {} {}]]",
                    rng.pick(WORDS),
                    rng.below(1000)
                );
            }
        };
        summary.links_wiki += 1;
        if by_name && name_only_ambiguous {
            summary.links_ambiguous += 1;
        }
        text
    }

    fn paragraph(&self, rng: &mut Rng, summary: &mut Summary, links: usize) -> String {
        let mut words: Vec<String> = (0..rng.range(12, 30))
            .map(|_| (*rng.pick(WORDS)).to_owned())
            .collect();
        words[0] = capitalize(&words[0]);
        for _ in 0..links {
            let at = rng.below(words.len() + 1);
            words.insert(at, self.link(rng, summary));
        }
        let mut p = words.join(" ");
        p.push('.');
        if rng.chance(25) {
            summary.inline_tags += 1;
            let _ = write!(p, " #{}", rng.pick(TAGS));
        }
        p
    }

    fn note_text(&self, i: usize, config: &Config, rng: &mut Rng, summary: &mut Summary) -> String {
        let meta = &self.metas[i];
        let mut t = String::new();

        if rng.chance(70) {
            t.push_str("---\n");
            let tag_count = rng.range(1, 2);
            t.push_str("tags:\n");
            for _ in 0..tag_count {
                let _ = writeln!(t, "  - {}", rng.pick(TAGS));
            }
            summary.frontmatter_tags += tag_count;
            if let Some(alias) = &meta.alias {
                let _ = writeln!(t, "aliases:\n  - {alias}");
            }
            let created = date_from_days(DAY_2024_01_01 + rng.below(1000));
            let _ = writeln!(t, "created: {created}");
            let _ = writeln!(t, "status: {}", rng.pick(STATUSES));
            let _ = writeln!(t, "priority: {}", rng.range(1, 5));
            let _ = writeln!(t, "done: {}", rng.chance(30));
            if rng.chance(10) {
                let j = rng.below(self.metas.len());
                let _ = writeln!(t, "related: \"[[{}]]\"", self.metas[j].stem);
                summary.links_frontmatter += 1;
                if self.is_ambiguous(j) {
                    summary.links_ambiguous += 1;
                }
            }
            t.push_str("---\n");
        } else if let Some(alias) = &meta.alias {
            // Aliases must be in frontmatter; keep them reachable.
            let _ = writeln!(t, "---\naliases:\n  - {alias}\n---");
        }

        if rng.chance(50) {
            let _ = writeln!(t, "# {}\n", meta.stem);
            summary.headings += 1;
        }

        let links = rng.range(0, 3);
        let _ = writeln!(t, "{} ^b{i}\n", self.paragraph(rng, summary, links));
        summary.blocks += 1;

        t.push_str("## Ringkasan\n\n");
        let links = rng.range(1, 3);
        let _ = writeln!(t, "{}\n", self.paragraph(rng, summary, links));

        t.push_str("## Detail\n\n");
        for _ in 0..rng.range(1, 3) {
            let links = rng.range(0, 2);
            let _ = writeln!(t, "{}\n", self.paragraph(rng, summary, links));
        }
        if rng.chance(30) {
            // Neither of these is a link or a tag: they sit in code.
            t.push_str("```text\n[[Bukan link]] #bukantag\n```\n\n");
            t.push_str("Contoh `[[juga bukan link]]` di kode inline.\n\n");
        }

        t.push_str("### Tindak lanjut\n\n");
        for _ in 0..rng.range(1, 4) {
            let mark = if rng.chance(40) { 'x' } else { ' ' };
            let _ = writeln!(
                t,
                "- [{mark}] {} {}",
                capitalize(rng.pick(WORDS)),
                rng.pick(WORDS)
            );
        }
        summary.headings += 3;

        if i == 0 && config.long_note_lines > 0 {
            let mut lines = t.matches('\n').count();
            let mut section = 0;
            while lines < config.long_note_lines {
                if lines.is_multiple_of(50) {
                    section += 1;
                    let _ = writeln!(t, "\n## Bagian {section}");
                    summary.headings += 1;
                    lines += 2;
                    continue;
                }
                let links = usize::from(lines.is_multiple_of(8));
                let _ = writeln!(t, "{}", self.paragraph(rng, summary, links));
                lines += 1;
            }
            summary.long_note_lines = t.matches('\n').count();
        }
        t
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Days from 1970-01-01 to 2024-01-01.
const DAY_2024_01_01: usize = 19_723;

/// Formats a day count since 1970-01-01 as `YYYY-MM-DD` (proleptic Gregorian).
fn date_from_days(days: usize) -> String {
    // Howard Hinnant's civil_from_days, for non-negative day counts.
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_correct() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(DAY_2024_01_01), "2024-01-01");
        assert_eq!(date_from_days(DAY_2024_01_01 + 59), "2024-02-29");
        assert_eq!(date_from_days(DAY_2024_01_01 + 366), "2025-01-01");
    }

    #[test]
    fn harian_index_matches_folder_list() {
        assert_eq!(FOLDERS[1], "Harian");
    }

    #[test]
    fn capitalize_handles_unicode() {
        assert_eq!(capitalize("über"), "Über");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn summary_json_is_stable() {
        let s = Summary {
            notes: 2,
            links_wiki: 3,
            links_embed: 1,
            ..Summary::default()
        };
        let json = s.to_json();
        assert!(json.starts_with("{\"v\":1,\"notes\":2,"));
        assert!(json.contains("\"links\":4,"));
    }
}
