//! `patinote mcp`, the read-only MCP server (A-04), driven over stdio by a
//! small JSON-RPC client.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

use common::{Env, hashes, json};
use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    /// Every line seen on stdout, to check it only carries protocol messages.
    seen: Vec<String>,
}

impl Client {
    fn start(env: &Env, vault: &str) -> Self {
        let mut child = env
            .cmd(&["mcp", "--vault", vault])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
            seen: Vec::new(),
        };
        let init = c.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
        );
        assert_eq!(init["serverInfo"]["name"], "patinote");
        c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        c
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed stdout"
            );
            self.seen.push(line.clone());
            let msg: Value = serde_json::from_str(&line).unwrap();
            if msg["id"] == id {
                return msg["result"].clone();
            }
        }
    }

    /// Calls a tool; returns (is_error, structured content).
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        // The same JSON is also sent as text.
        let text = r["content"][0]["text"].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            r["structuredContent"]
        );
        (r["isError"] == true, r["structuredContent"].clone())
    }

    fn close(mut self) -> Vec<String> {
        drop(self.stdin);
        let mut rest = String::new();
        while self.stdout.read_line(&mut rest).unwrap() > 0 {
            self.seen.push(std::mem::take(&mut rest));
        }
        assert!(self.child.wait().unwrap().success());
        self.seen
    }
}

/// Each tool with arguments, and the CLI command it mirrors.
fn pairs() -> Vec<(&'static str, Value, Vec<&'static str>)> {
    vec![
        (
            "search_notes",
            json!({"query": "indeks"}),
            vec!["search", "indeks"],
        ),
        (
            "read_note",
            json!({"note": "Patinote"}),
            vec!["read", "Patinote"],
        ),
        (
            "list_notes",
            json!({"tag": "proyek", "sort": "modified"}),
            vec!["list", "--tag", "proyek", "--sort", "modified"],
        ),
        (
            "get_backlinks",
            json!({"note": "Second Brain"}),
            vec!["backlinks", "Second Brain"],
        ),
        (
            "get_links",
            json!({"note": "Desain indeks Patinote"}),
            vec!["links", "Desain indeks Patinote"],
        ),
        ("list_tags", json!({}), vec!["tags"]),
        ("find_orphans", json!({}), vec!["orphans"]),
        ("list_unresolved", json!({}), vec!["unresolved"]),
        (
            "resolve_link",
            json!({"name": "[[Rapat]]"}),
            vec!["resolve", "[[Rapat]]"],
        ),
        ("vault_info", json!({}), vec!["vault", "info"]),
        ("list_conflicts", json!({}), vec!["conflicts"]),
    ]
}

#[test]
fn a04_1_the_eleven_tools_are_listed_with_schemas() {
    let env = Env::new();
    let vault = env.fixture();
    let mut c = Client::start(&env, vault.to_str().unwrap());
    let tools = c.request("tools/list", json!({}))["tools"].clone();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut expected: Vec<&str> = pairs().iter().map(|p| p.0).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    expected.sort_unstable();
    assert_eq!(sorted, expected);
    for t in tools.as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(
            t["description"].as_str().unwrap().len() > 40,
            "{}",
            t["name"]
        );
    }
    c.close();
}

#[test]
fn a04_2_tool_results_equal_the_cli_json() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    // Index once so neither side sees a first-run difference.
    assert_eq!(env.run(&["--vault", v, "index"]).code, 0);
    let mut c = Client::start(&env, v);
    for (tool, args, cli) in pairs() {
        let (is_error, got) = c.call(tool, args);
        assert!(!is_error, "{tool}: {got}");
        let mut all = vec!["--vault", v];
        all.extend(cli);
        all.push("--json");
        let r = env.run(&all);
        assert_eq!(r.code, 0, "{tool}: {}", r.stderr);
        assert_eq!(got, json(&r.stdout), "{tool}");
    }
    c.close();
}

#[test]
fn a04_4_a_session_calling_every_tool_changes_nothing_in_the_vault() {
    let env = Env::new();
    let vault = env.fixture();
    let before = hashes(&vault);
    let mut c = Client::start(&env, vault.to_str().unwrap());
    for (tool, args, _) in pairs() {
        c.call(tool, args);
    }
    c.close();
    assert_eq!(hashes(&vault), before);
}

#[test]
fn a04_5_paths_outside_the_vault_are_rejected() {
    let env = Env::new();
    let vault = env.fixture();
    let mut c = Client::start(&env, vault.to_str().unwrap());
    for note in ["../../etc/passwd", "/etc/passwd"] {
        let (is_error, doc) = c.call("read_note", json!({"note": note}));
        assert!(is_error);
        assert_eq!(doc["error"]["code"], "invalid_argument", "{note}");
    }
    c.close();
}

#[test]
fn a04_6_a_300_kb_note_reads_in_three_parts_without_gaps_or_overlap() {
    let env = Env::new();
    let vault = env.fixture();
    let lines: Vec<String> = (1..=3000)
        .map(|i| format!("{i:05} {}", "x".repeat(93)))
        .collect();
    let original = lines.join("\n");
    env.write("vault/Panjang.md", &(original.clone() + "\n"));
    assert_eq!(original.len() + 1, 300_000);
    let mut c = Client::start(&env, vault.to_str().unwrap());
    let mut parts = Vec::new();
    let mut from = 1;
    loop {
        let (is_error, doc) = c.call(
            "read_note",
            json!({"note": "Panjang", "lines": format!("{from}:")}),
        );
        assert!(!is_error, "{doc}");
        assert!(doc["content"].as_str().unwrap().len() <= 100 * 1024);
        parts.push(doc["content"].as_str().unwrap().to_owned());
        if doc["truncated"] == false {
            break;
        }
        from = doc["lines"]["end"].as_u64().unwrap() as usize + 1;
    }
    assert_eq!(parts.len(), 3);
    assert_eq!(parts.join("\n"), original);
    c.close();
}

#[test]
fn a04_7_stdout_only_carries_protocol_messages() {
    let env = Env::new();
    let vault = env.fixture();
    let mut c = Client::start(&env, vault.to_str().unwrap());
    for (tool, args, _) in pairs() {
        c.call(tool, args);
    }
    c.call("read_note", json!({"note": "Rapat"}));
    for line in c.close() {
        let msg: Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC ({e}): {line}"));
        assert_eq!(msg["jsonrpc"], "2.0", "{line}");
    }
}

#[test]
fn errors_are_the_cli_error_document() {
    let env = Env::new();
    let vault = env.fixture();
    let mut c = Client::start(&env, vault.to_str().unwrap());
    let (is_error, doc) = c.call("read_note", json!({"note": "Rapat"}));
    assert!(is_error);
    assert_eq!(doc["v"], 1);
    assert_eq!(doc["error"]["code"], "ambiguous_note");
    assert_eq!(doc["error"]["candidates"].as_array().unwrap().len(), 2);
    let (is_error, doc) = c.call("list_notes", json!({"bogus": 1}));
    assert!(is_error);
    assert_eq!(doc["error"]["code"], "invalid_argument");
    c.close();
}

/// Configuration warnings are part of a command's JSON; the tool gives them
/// too, on every call (A-04: identical to the CLI).
#[test]
fn a04_2_config_warnings_are_in_tool_results_as_in_the_cli() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    env.write("config/patinote/config.toml", "kunci_asing = 1\n");
    assert_eq!(env.run(&["--vault", v, "index"]).code, 0);
    let cli = json(&env.run(&["--vault", v, "vault", "info", "--json"]).stdout);
    assert!(
        cli["warnings"]
            .as_array()
            .is_some_and(|w| w.iter().any(|w| w["code"] == "config")),
        "{cli}"
    );
    let mut c = Client::start(&env, v);
    for _ in 0..2 {
        let (is_error, got) = c.call("vault_info", json!({}));
        assert!(!is_error);
        assert_eq!(got, cli);
    }
    c.close();
}
