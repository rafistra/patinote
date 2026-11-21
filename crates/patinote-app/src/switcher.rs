//! Fuzzy finding notes by title, alias and folder (F-07), shared with link
//! autocomplete (F-08). Pure logic: the UI is in `screens/switcher.rs`.
//!
//! Matching uses `nucleo-matcher` with case and diacritics ignored ("cafe"
//! finds "Café", "PATI" finds "Patinote"). Results are ordered by match
//! score, then a bonus for recently opened notes, then shorter paths.

use std::collections::HashMap;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};

/// At most this many results are shown (F-07).
pub const MAX_RESULTS: usize = 50;

/// Score added for the most recently opened note, decreasing with age.
const RECENT_BONUS: u32 = 64;
/// Score taken off a match on an alias or the folder path, so that a title
/// that matches as well comes first.
const SECONDARY_PENALTY: u32 = 8;

/// A note that can be found.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Path relative to the vault.
    pub path: String,
    /// The file name without `.md`.
    pub title: String,
    /// Folder with a trailing `/`, or empty at the vault root.
    pub folder: String,
    pub aliases: Vec<String>,
    haystack_title: Utf32String,
    haystack_path: Utf32String,
    haystack_aliases: Vec<Utf32String>,
}

impl Candidate {
    /// A candidate for the note at `path`.
    pub fn new(path: &str, aliases: Vec<String>) -> Self {
        let (folder, file) = match path.rsplit_once('/') {
            Some((dir, file)) => (format!("{dir}/"), file),
            None => (String::new(), path),
        };
        let title = file.strip_suffix(".md").unwrap_or(file).to_owned();
        Self {
            haystack_title: Utf32String::from(title.as_str()),
            haystack_path: Utf32String::from(format!("{folder}{title}").as_str()),
            haystack_aliases: aliases
                .iter()
                .map(|a| Utf32String::from(a.as_str()))
                .collect(),
            path: path.to_owned(),
            title,
            folder,
            aliases,
        }
    }
}

/// What matched in a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matched {
    /// The title; positions of matched characters in it.
    Title(Vec<usize>),
    /// An alias (index into `aliases`); positions in the alias.
    Alias(usize, Vec<usize>),
}

/// One result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Index into the candidates.
    pub candidate: usize,
    pub matched: Matched,
    pub score: u32,
}

/// The ranked results for a query, and how many candidates matched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub total_matches: usize,
}

/// Finds notes. Keeps the matcher's buffers between queries.
pub struct Finder {
    matcher: Matcher,
}

impl Default for Finder {
    fn default() -> Self {
        Self::new()
    }
}

impl Finder {
    /// A finder configured for file names.
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    /// Ranks `candidates` for `query`. `recent` maps paths to how recently
    /// they were opened (0 = most recent). An empty query lists the recent
    /// notes, newest first (F-07.4).
    pub fn find(
        &mut self,
        candidates: &[Candidate],
        query: &str,
        recent: &HashMap<String, usize>,
    ) -> Results {
        if query.trim().is_empty() {
            let mut hits: Vec<_> = candidates
                .iter()
                .enumerate()
                .filter_map(|(i, c)| recent.get(&c.path).map(|r| (*r, i)))
                .collect();
            hits.sort_unstable();
            let total_matches = hits.len();
            return Results {
                hits: hits
                    .into_iter()
                    .take(MAX_RESULTS)
                    .map(|(_, i)| Hit {
                        candidate: i,
                        matched: Matched::Title(Vec::new()),
                        score: 0,
                    })
                    .collect(),
                total_matches,
            };
        }
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut scored: Vec<(u32, usize, usize)> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            let secondary = |s: Option<u32>| s.map(|s| s.saturating_sub(SECONDARY_PENALTY));
            let mut best = pattern.score(c.haystack_title.slice(..), &mut self.matcher);
            for a in &c.haystack_aliases {
                best = best.max(secondary(pattern.score(a.slice(..), &mut self.matcher)));
            }
            if !c.folder.is_empty() {
                best = best.max(secondary(
                    pattern.score(c.haystack_path.slice(..), &mut self.matcher),
                ));
            }
            let Some(score) = best else { continue };
            let bonus = recent
                .get(&c.path)
                .map_or(0, |r| RECENT_BONUS.saturating_sub(*r as u32 * 4));
            scored.push((score + bonus, c.path.len(), i));
        }
        let total_matches = scored.len();
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        scored.truncate(MAX_RESULTS);
        let hits = scored
            .into_iter()
            .map(|(score, _, i)| Hit {
                candidate: i,
                matched: self.highlight(&pattern, &candidates[i]),
                score,
            })
            .collect();
        Results {
            hits,
            total_matches,
        }
    }

    /// Which text matched best, with the positions to highlight.
    fn highlight(&mut self, pattern: &Pattern, c: &Candidate) -> Matched {
        let mut indices = Vec::new();
        let title = pattern.indices(c.haystack_title.slice(..), &mut self.matcher, &mut indices);
        let title_indices = sorted(&mut indices);
        let mut best: (Option<u32>, Matched) = (title, Matched::Title(title_indices));
        for (n, a) in c.haystack_aliases.iter().enumerate() {
            let s = pattern.indices(a.slice(..), &mut self.matcher, &mut indices);
            if s > best.0 {
                best = (s, Matched::Alias(n, sorted(&mut indices)));
            }
        }
        if best.0.is_none() && !c.folder.is_empty() {
            // Only the folder path matched: highlight what falls in the title.
            pattern.indices(c.haystack_path.slice(..), &mut self.matcher, &mut indices);
            let offset = c.folder.chars().count();
            let in_title = sorted(&mut indices)
                .into_iter()
                .filter_map(|i| i.checked_sub(offset))
                .collect();
            best.1 = Matched::Title(in_title);
        }
        best.1
    }
}

fn sorted(indices: &mut Vec<u32>) -> Vec<usize> {
    indices.sort_unstable();
    indices.dedup();
    let out = indices.iter().map(|&i| i as usize).collect();
    indices.clear();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<Candidate> {
        vec![
            Candidate::new("Proyek/Desain indeks Patinote.md", vec![]),
            Candidate::new("Proyek/Patinote.md", vec!["Second Brain".into()]),
            Candidate::new("Referensi/Daftar sumber indeks.md", vec![]),
            Candidate::new("Proyek/Patinote/Desain antarmuka induk.md", vec![]),
            Candidate::new("Arsip/Skema indeks lama.md", vec!["Design index".into()]),
            Candidate::new("Café Bandung.md", vec![]),
        ]
    }

    fn titles(c: &[Candidate], r: &Results) -> Vec<String> {
        r.hits
            .iter()
            .map(|h| c[h.candidate].title.clone())
            .collect()
    }

    #[test]
    fn dsn_ind_finds_the_design_notes_first() {
        let c = candidates();
        let r = Finder::new().find(&c, "dsn ind", &HashMap::new());
        assert_eq!(titles(&c, &r)[0], "Desain indeks Patinote");
        assert!(r.total_matches >= 3);
        let Matched::Title(idx) = &r.hits[0].matched else {
            panic!("title match")
        };
        assert!(idx.contains(&0) && idx.contains(&7), "{idx:?}");
    }

    #[test]
    fn f07_2_an_alias_finds_its_note() {
        let c = candidates();
        let r = Finder::new().find(&c, "second brain", &HashMap::new());
        assert_eq!(titles(&c, &r)[0], "Patinote");
        assert!(matches!(r.hits[0].matched, Matched::Alias(0, _)));
    }

    #[test]
    fn f07_3_case_and_diacritics_are_ignored() {
        let c = candidates();
        let mut f = Finder::new();
        assert_eq!(
            titles(&c, &f.find(&c, "cafe", &HashMap::new()))[0],
            "Café Bandung"
        );
        assert!(titles(&c, &f.find(&c, "PATI", &HashMap::new())).contains(&"Patinote".to_owned()));
    }

    #[test]
    fn f07_4_an_empty_query_lists_recent_notes_newest_first() {
        let c = candidates();
        let recent = HashMap::from([
            ("Proyek/Patinote.md".to_owned(), 1),
            ("Café Bandung.md".to_owned(), 0),
        ]);
        let r = Finder::new().find(&c, "", &recent);
        assert_eq!(titles(&c, &r), ["Café Bandung", "Patinote"]);
    }

    #[test]
    fn recent_notes_rank_higher_and_ties_prefer_shorter_paths() {
        let c = vec![
            Candidate::new("a/long/folder/Rapat.md", vec![]),
            Candidate::new("x/Rapat.md", vec![]),
        ];
        let mut f = Finder::new();
        assert_eq!(f.find(&c, "rapat", &HashMap::new()).hits[0].candidate, 1);
        let recent = HashMap::from([("a/long/folder/Rapat.md".to_owned(), 0)]);
        assert_eq!(f.find(&c, "rapat", &recent).hits[0].candidate, 0);
    }

    #[test]
    fn a_folder_query_finds_notes_in_it() {
        let c = candidates();
        let r = Finder::new().find(&c, "arsip skema", &HashMap::new());
        assert_eq!(titles(&c, &r)[0], "Skema indeks lama");
    }
}
