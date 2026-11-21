//! `patinote agents init`: writes `AGENTS.md` in the vault so AI agents work
//! through Patinote and follow the vault's conventions (PRD A-05).
//!
//! The generated part sits between two markers; `--update` replaces only that
//! part, so the user's own notes below it never change (A-05.2). Writing goes
//! through the core write path, journaled, as actor `cli`.

use std::collections::BTreeMap;

use patinote_core::commands::{AgentsInitOutput, ErrorCode};
use patinote_core::names::stem;
use patinote_core::write::{
    Actor, Expect, Op, Transaction, WriteError, commit, content_hash, recover,
};

use crate::commands::Ctx;
use crate::context::{index_failure, io_failure, warning};
use crate::output::Failure;

/// Start of the generated part.
pub const BEGIN: &str = "<!-- patinote:begin -->";
/// End of the generated part.
pub const END: &str = "<!-- patinote:end -->";

/// Agents that get a short pointer file (`--for`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Agent {
    /// `CLAUDE.md` for Claude Code.
    Claude,
    /// `GEMINI.md` for Gemini CLI.
    Gemini,
}

impl Agent {
    fn file(self) -> &'static str {
        match self {
            Self::Claude => "CLAUDE.md",
            Self::Gemini => "GEMINI.md",
        }
    }
}

/// Conventions detected from the index.
struct Conventions {
    folders: Vec<(String, usize)>,
    tags: Vec<(String, usize)>,
    daily: Option<String>,
    attachments: Option<String>,
    wikilinks: usize,
    markdown_links: usize,
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn detect(ctx: &Ctx<'_>) -> Result<Conventions, Failure> {
    let notes = ctx.store.all_notes().map_err(index_failure)?;
    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
    let mut daily: BTreeMap<String, usize> = BTreeMap::new();
    let mut attachments: BTreeMap<String, usize> = BTreeMap::new();
    for n in notes.iter().filter(|n| !n.excluded) {
        match n.kind.as_str() {
            "note" => {
                if let Some((top, _)) = n.path.split_once('/') {
                    *folders.entry(top.to_owned()).or_default() += 1;
                }
                if is_date(stem(&n.path)) {
                    *daily.entry(parent(&n.path).to_owned()).or_default() += 1;
                }
            }
            "attachment" => *attachments.entry(parent(&n.path).to_owned()).or_default() += 1,
            _ => {}
        }
    }
    let top = |m: BTreeMap<String, usize>, n: usize| {
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    };
    let mut tags: Vec<(String, usize)> = ctx
        .store
        .tag_counts()
        .map_err(index_failure)?
        .into_iter()
        .map(|t| (t.display, t.uses))
        .collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    tags.truncate(20);
    let (mut wikilinks, mut markdown_links) = (0, 0);
    for n in notes.iter().filter(|n| n.kind == "note" && !n.excluded) {
        for l in ctx.store.outgoing(n.id).map_err(index_failure)? {
            match l.kind.as_str() {
                "markdown" => markdown_links += 1,
                _ => wikilinks += 1,
            }
        }
    }
    Ok(Conventions {
        folders: top(folders, 6),
        tags,
        daily: top(daily, 1)
            .into_iter()
            .find(|(_, n)| *n >= 2)
            .map(|(d, _)| d),
        attachments: top(attachments, 1).into_iter().next().map(|(d, _)| d),
        wikilinks,
        markdown_links,
    })
}

fn folder(d: &str) -> String {
    if d.is_empty() {
        "the vault root".into()
    } else {
        format!("`{d}/`")
    }
}

/// The generated part of AGENTS.md, markers included.
fn generated(c: &Conventions) -> String {
    let mut s = String::new();
    s.push_str(BEGIN);
    s.push_str("\n## Working in this vault\n\n");
    s.push_str(
        "This is a Markdown vault managed with Patinote (compatible with Obsidian).\n\n\
         - Search and read notes through Patinote: the `patinote` CLI or its MCP server (`patinote mcp`). \
         Prefer `patinote search`, `patinote read` and `patinote backlinks` over `grep`, `find` or opening files.\n\
         - Do not rename, move or delete notes yourself: links in other notes would break. Ask the user to do it in Patinote.\n\
         - A note is named by its name, an alias, or its path relative to the vault.\n",
    );
    s.push_str("\n## Vault conventions\n\n");
    if !c.folders.is_empty() {
        let list: Vec<String> = c
            .folders
            .iter()
            .map(|(f, n)| format!("`{f}/` ({n})"))
            .collect();
        s.push_str(&format!("- Main folders: {}.\n", list.join(", ")));
    }
    if !c.tags.is_empty() {
        let list: Vec<String> = c
            .tags
            .iter()
            .map(|(t, n)| format!("`#{t}` ({n})"))
            .collect();
        s.push_str(&format!("- Most used tags: {}.\n", list.join(", ")));
    }
    if let Some(d) = &c.daily {
        let path = if d.is_empty() {
            "YYYY-MM-DD.md".to_owned()
        } else {
            format!("{d}/YYYY-MM-DD.md")
        };
        s.push_str(&format!("- Daily notes: `{path}`.\n"));
    }
    if let Some(a) = &c.attachments {
        s.push_str(&format!("- Attachments are kept in {}.\n", folder(a)));
    }
    if c.wikilinks >= c.markdown_links {
        s.push_str("- Links are wikilinks: `[[Note]]`, `[[Note#Heading]]`, `[[Note|text]]`.\n");
    } else {
        s.push_str("- Links are Markdown links: `[text](Folder/Note.md)`.\n");
    }
    s.push_str(
        "\n## Writing rules\n\n\
         - Keep a note's frontmatter (the `---` block at the top) as it is, and add properties there.\n\
         - Link with the shortest name that is unique; add the folder only when two notes share a name.\n\
         - Never change anything in `.obsidian/` or `.trash/`.\n",
    );
    s.push_str(
        "\n## Commands\n\n\
         | Task | CLI | MCP tool |\n\
         | --- | --- | --- |\n\
         | Search text | `patinote search \"words\" --json` | `search_notes` |\n\
         | Read a note | `patinote read <note> --json` | `read_note` |\n\
         | List notes | `patinote list --tag T --folder F --json` | `list_notes` |\n\
         | Links to a note | `patinote backlinks <note> --json` | `get_backlinks` |\n\
         | Links in a note | `patinote links <note> --json` | `get_links` |\n\
         | Where a name points | `patinote resolve <name> --json` | `resolve_link` |\n\
         \n\
         Exit codes: 0 ok, 1 error, 2 bad arguments, 3 not found, 4 ambiguous name (the error lists the \
         candidates; use a path), 5 index locked, 130 interrupted.\n",
    );
    s.push_str(END);
    s.push('\n');
    s
}

const PERSONAL: &str = "\n## Personal notes\n\n\
    Write your own instructions for agents here. `patinote agents init --update` never changes this part.\n";

fn pointer(agent: Agent) -> String {
    format!(
        "# {}\n\n@AGENTS.md\n\nThe instructions for this vault are in AGENTS.md (written by `patinote agents init`).\n",
        agent.file()
    )
}

fn write_error(e: WriteError) -> Failure {
    match e {
        WriteError::Conflict { paths } => Failure::new(
            ErrorCode::AlreadyExists,
            format!(
                "{} changed while writing; run the command again",
                paths.join(", ")
            ),
        ),
        other => io_failure("cannot write", other),
    }
}

/// Runs `agents init`.
pub fn init(ctx: &Ctx<'_>, update: bool, agents: &[Agent]) -> Result<AgentsInitOutput, Failure> {
    let root = &ctx.vault.root;
    // Writers finish undoing interrupted writes first.
    recover(root, &ctx.vault.journal).map_err(|e| io_failure("cannot check the journal", e))?;

    let path = root.join("AGENTS.md");
    let existing = match std::fs::read(&path) {
        Ok(bytes) => Some(
            String::from_utf8(bytes)
                .map_err(|_| Failure::new(ErrorCode::NotUtf8, "AGENTS.md is not valid UTF-8"))?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_failure("cannot read AGENTS.md", e)),
    };
    let fresh = generated(&detect(ctx)?);
    let mut ops = Vec::new();
    let mut out = AgentsInitOutput {
        v: 1,
        written: vec![],
        unchanged: vec![],
        warnings: vec![],
    };

    match (&existing, update) {
        (Some(_), false) => {
            return Err(Failure::new(
                ErrorCode::AlreadyExists,
                "AGENTS.md already exists; run `patinote agents init --update` to refresh the generated part",
            ));
        }
        (None, true) => {
            return Err(Failure::new(
                ErrorCode::NoteNotFound,
                "AGENTS.md does not exist yet; run `patinote agents init`",
            ));
        }
        (None, false) => {
            let text = format!("# AGENTS.md\n\n{fresh}{PERSONAL}");
            ops.push(Op::Write {
                path: "AGENTS.md".into(),
                content: text.into_bytes(),
                expect: Expect::Absent,
            });
            out.written.push("AGENTS.md".into());
        }
        (Some(old), true) => {
            // The end marker that closes the generated part: the first one
            // after the begin marker, not one mentioned above it.
            let begin = old.find(BEGIN);
            let end_marker = begin.and_then(|b| old[b..].find(END).map(|i| b + i));
            let (Some(b), Some(e)) = (begin, end_marker) else {
                return Err(Failure::new(
                    ErrorCode::InvalidArgument,
                    format!(
                        "AGENTS.md has no {BEGIN} … {END} markers; add them around the part Patinote may replace"
                    ),
                ));
            };
            // Replace exactly the marked part, end marker and its newline included.
            let mut end = e + END.len();
            if old[end..].starts_with('\n') {
                end += 1;
            }
            let new = format!("{}{fresh}{}", &old[..b], &old[end..]);
            if new == *old {
                out.unchanged.push("AGENTS.md".into());
            } else {
                // The text the new one was built from must still be on disk.
                ops.push(Op::Write {
                    path: "AGENTS.md".into(),
                    content: new.into_bytes(),
                    expect: Expect::Hash(content_hash(old.as_bytes())),
                });
                out.written.push("AGENTS.md".into());
            }
        }
    }

    for agent in agents {
        let file = agent.file();
        match std::fs::read_to_string(root.join(file)) {
            Ok(text) if text.contains("AGENTS.md") => out.unchanged.push(file.into()),
            Ok(_) => out.warnings.push(warning(
                "not_written",
                Some(file),
                None,
                format!(
                    "{file} exists and does not mention AGENTS.md; add `@AGENTS.md` to it yourself"
                ),
            )),
            Err(_) => {
                ops.push(Op::Write {
                    path: file.into(),
                    content: pointer(*agent).into_bytes(),
                    expect: Expect::Absent,
                });
                out.written.push(file.into());
            }
        }
    }

    if !ops.is_empty() {
        commit(
            root,
            &ctx.vault.journal,
            &Transaction {
                actor: Actor::Cli,
                ops,
            },
        )
        .map_err(write_error)?;
    }
    Ok(out)
}
