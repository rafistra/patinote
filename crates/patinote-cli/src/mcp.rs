//! `patinote mcp`: a read-only MCP server over stdio (PRD A-04).
//!
//! Every tool calls the same command function as the CLI, so its result is
//! the CLI's `--json` document (A-04.2), sent as structured content and as
//! text. No tool writes to the vault; the only file written is the index
//! cache. stdout carries protocol messages only; logs go to stderr.

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use patinote_core::commands::{ErrorBody, ErrorCode, ErrorOutput};
use patinote_index::SqliteStore;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, InitializeResult,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::commands::{self, Ctx, KindFilter, ListArgs, SortBy};
use crate::context::{Vault, refresh};
use crate::output::Failure;

/// `read_note` returns at most this much text per call (A-04).
pub const READ_LIMIT_BYTES: usize = 100 * 1024;

/// Never set: the server is stopped by the client closing stdin.
static NEVER: AtomicBool = AtomicBool::new(false);

/// Runs the server until the client closes stdin.
pub fn serve(vault: Vault, store: SqliteStore) -> Result<(), Failure> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| crate::context::io_failure("cannot start the MCP server", e))?;
    let server = Server {
        state: Mutex::new((vault, store)),
    };
    runtime.block_on(async move {
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| crate::context::io_failure("MCP handshake failed", e))?;
        service
            .waiting()
            .await
            .map_err(|e| crate::context::io_failure("MCP server stopped", e))?;
        Ok(())
    })
}

struct Server {
    state: Mutex<(Vault, SqliteStore)>,
}

// ---------------------------------------------------------------------------
// Tool arguments. Doc comments become the descriptions agents see.

/// Arguments of `search_notes`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    /// Words and "quoted phrases"; every one must appear. Example: `indeks "skema SQLite"`.
    query: String,
    /// Maximum results (default 50, 0 for all).
    limit: Option<usize>,
}

/// Arguments of `read_note`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    /// Note name, alias or vault-relative path. Example: `Patinote` or `Proyek/Patinote.md`.
    note: String,
    /// Lines to read, 1-based and inclusive, like `"1:200"` or `"201:"`. A call returns at
    /// most 100 KB; when `truncated` is true, continue from `lines.end + 1`.
    lines: Option<String>,
}

/// Arguments of `list_notes`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListToolArgs {
    /// Only notes with this tag or a tag nested under it. Example: `proyek`.
    tag: Option<String>,
    /// Only notes in this folder and its subfolders. Example: `Harian`.
    folder: Option<String>,
    /// `note` (default), `attachment`, `canvas` or `all`.
    kind: Option<String>,
    /// `name` (default) or `modified` (newest first).
    sort: Option<String>,
    /// Maximum results (default 50, 0 for all).
    limit: Option<usize>,
}

/// A note name, alias or path.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoteArg {
    /// Note name, alias or vault-relative path. Example: `Patinote`.
    note: String,
}

/// Arguments of `list_tags`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TagArgs {
    /// Only this tag and the tags nested under it. Example: `proyek`.
    prefix: Option<String>,
}

/// Arguments of `resolve_link`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResolveArgs {
    /// A name, alias, path or `[[link]]`. Example: `[[Rapat]]`.
    name: String,
}

/// No arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

fn schema<T: JsonSchema>() -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    let value = serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default();
    std::sync::Arc::new(value.as_object().cloned().unwrap_or_default())
}

/// The 11 tools (A-04), each mirroring a CLI command.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "search_notes",
            "Search note text for words and phrases, best matches first. Same as `patinote search`. Use it to find notes about a topic.",
            schema::<SearchArgs>(),
        ),
        Tool::new(
            "read_note",
            "Read a note's text with its frontmatter, tags, headings and links. Same as `patinote read`. At most 100 KB per call; read long notes in parts with `lines`.",
            schema::<ReadArgs>(),
        ),
        Tool::new(
            "list_notes",
            "List notes, filtered by tag, folder or kind. Same as `patinote list`. Use it to see what exists in a folder or under a tag.",
            schema::<ListToolArgs>(),
        ),
        Tool::new(
            "get_backlinks",
            "Notes that link to a note, with the line around each link. Same as `patinote backlinks`. Use it to see how a note is connected.",
            schema::<NoteArg>(),
        ),
        Tool::new(
            "get_links",
            "Links written in a note, including to notes that do not exist yet. Same as `patinote links`.",
            schema::<NoteArg>(),
        ),
        Tool::new(
            "list_tags",
            "Tags with how often they are used, nested tags included. Same as `patinote tags`.",
            schema::<TagArgs>(),
        ),
        Tool::new(
            "find_orphans",
            "Notes with no links in or out. Same as `patinote orphans`.",
            schema::<NoArgs>(),
        ),
        Tool::new(
            "list_unresolved",
            "Links to notes that do not exist, grouped by name. Same as `patinote unresolved`.",
            schema::<NoArgs>(),
        ),
        Tool::new(
            "resolve_link",
            "Which note a name, alias, path or [[link]] points to, and whether it is ambiguous. Same as `patinote resolve`.",
            schema::<ResolveArgs>(),
        ),
        Tool::new(
            "vault_info",
            "Vault and index location, file counts and warnings. Same as `patinote vault info`.",
            schema::<NoArgs>(),
        ),
        Tool::new(
            "list_conflicts",
            "Syncthing conflict copies and the notes they conflict with. Same as `patinote conflicts`.",
            schema::<NoArgs>(),
        ),
    ]
}

fn args<T: DeserializeOwned>(params: &CallToolRequestParams) -> Result<T, Failure> {
    let value = serde_json::Value::Object(params.arguments.clone().unwrap_or_default());
    serde_json::from_value(value).map_err(|e| {
        Failure::new(
            ErrorCode::InvalidArgument,
            format!("invalid arguments: {e}"),
        )
    })
}

fn kind(s: Option<&str>) -> Result<KindFilter, Failure> {
    Ok(match s.unwrap_or("note") {
        "note" => KindFilter::Note,
        "attachment" => KindFilter::Attachment,
        "canvas" => KindFilter::Canvas,
        "all" => KindFilter::All,
        other => {
            return Err(Failure::new(
                ErrorCode::InvalidArgument,
                format!("unknown kind: {other}"),
            ));
        }
    })
}

fn sort(s: Option<&str>) -> Result<SortBy, Failure> {
    Ok(match s.unwrap_or("name") {
        "name" => SortBy::Name,
        "modified" => SortBy::Modified,
        other => {
            return Err(Failure::new(
                ErrorCode::InvalidArgument,
                format!("unknown sort: {other}"),
            ));
        }
    })
}

/// Serializes an output with the refresh warnings added, like the CLI does.
fn json<T: serde::Serialize>(
    mut out: T,
    vault: &Vault,
    push: impl FnOnce(&mut T, Vec<patinote_core::commands::OutputWarning>),
) -> serde_json::Value {
    push(&mut out, vault.warnings.clone());
    serde_json::to_value(out).unwrap_or_default()
}

macro_rules! with_warnings {
    () => {
        |o, w| o.warnings.extend(w)
    };
}

impl Server {
    /// Refreshes the index (A-04: on every call) and runs one tool.
    fn call(&self, params: &CallToolRequestParams) -> Result<serde_json::Value, Failure> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| Failure::new(ErrorCode::IoError, "the server state is unavailable"))?;
        let (vault, store) = &mut *guard;
        vault.warnings.clear();
        refresh(vault, store, true, &NEVER)?;
        let ctx = Ctx { vault, store };
        Ok(match params.name.as_ref() {
            "search_notes" => {
                let a: SearchArgs = args(params)?;
                json(
                    commands::search(&ctx, &a.query, a.limit.unwrap_or(commands::DEFAULT_LIMIT))?,
                    vault,
                    with_warnings!(),
                )
            }
            "read_note" => {
                let a: ReadArgs = args(params)?;
                let range = match &a.lines {
                    Some(spec) => commands::parse_lines(spec)?,
                    None => (None, None),
                };
                let row = commands::resolve_note(&ctx, &a.note)?;
                json(
                    commands::read(&ctx, &row, range, Some(READ_LIMIT_BYTES))?,
                    vault,
                    with_warnings!(),
                )
            }
            "list_notes" => {
                let a: ListToolArgs = args(params)?;
                let list = ListArgs {
                    tag: a.tag.as_deref(),
                    folder: a.folder.as_deref(),
                    kind: kind(a.kind.as_deref())?,
                    sort: sort(a.sort.as_deref())?,
                    limit: a.limit.unwrap_or(commands::DEFAULT_LIMIT),
                };
                json(commands::list(&ctx, &list)?, vault, with_warnings!())
            }
            "get_backlinks" => {
                let a: NoteArg = args(params)?;
                json(
                    commands::backlinks(&ctx, &commands::resolve_note(&ctx, &a.note)?)?,
                    vault,
                    with_warnings!(),
                )
            }
            "get_links" => {
                let a: NoteArg = args(params)?;
                json(
                    commands::links(&ctx, &commands::resolve_note(&ctx, &a.note)?)?,
                    vault,
                    with_warnings!(),
                )
            }
            "list_tags" => {
                let a: TagArgs = args(params)?;
                json(
                    commands::tags(&ctx, a.prefix.as_deref())?,
                    vault,
                    with_warnings!(),
                )
            }
            "find_orphans" => {
                let _: NoArgs = args(params)?;
                json(commands::orphans(&ctx)?, vault, with_warnings!())
            }
            "list_unresolved" => {
                let _: NoArgs = args(params)?;
                json(commands::unresolved(&ctx)?, vault, with_warnings!())
            }
            "resolve_link" => {
                let a: ResolveArgs = args(params)?;
                json(commands::resolve(&ctx, &a.name)?, vault, with_warnings!())
            }
            "vault_info" => {
                let _: NoArgs = args(params)?;
                json(commands::vault_info(&ctx)?, vault, with_warnings!())
            }
            "list_conflicts" => {
                let _: NoArgs = args(params)?;
                json(commands::conflicts(&ctx)?, vault, with_warnings!())
            }
            other => {
                return Err(Failure::new(
                    ErrorCode::InvalidArgument,
                    format!("unknown tool: {other}"),
                ));
            }
        })
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("patinote", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Read-only access to one Patinote vault (Obsidian-compatible Markdown notes). \
                 Find notes with search_notes or list_notes, read them with read_note, and follow \
                 connections with get_backlinks and get_links. Notes are named by name, alias or path. \
                 No tool changes the vault.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let result = match self.call(&request) {
            Ok(value) => CallToolResult::structured(value),
            Err(Failure::Error(body)) => error_result(body),
            Err(Failure::Interrupted | Failure::BrokenPipe) => error_result(ErrorBody {
                code: ErrorCode::IoError,
                message: "interrupted".into(),
                candidates: None,
            }),
        };
        Ok(CallToolResponse::Complete(result))
    }
}

/// The CLI's JSON error document as a tool error (A-02, A-04).
fn error_result(body: ErrorBody) -> CallToolResult {
    let doc = ErrorOutput { v: 1, error: body };
    CallToolResult::structured_error(serde_json::to_value(doc).unwrap_or_default())
}
