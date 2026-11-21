//! The global configuration file, `$XDG_CONFIG_HOME/patinote/config.toml`
//! (PRD "Gerbang 2: dampak arsitektur", point 3).
//!
//! Unknown keys give warnings, not errors, so an older Patinote can read a
//! newer file. Read so far: `[vault] default` (step 1.7), and `layout`,
//! `renderer` and `[editor] insert_escape` and `relative_numbers` for the
//! desktop app (Phase 2); the other sections are accepted and read by later
//! steps.

use std::path::{Path, PathBuf};

/// The configuration format this version writes.
pub const CONFIG_VERSION: i64 = 1;

/// Sections known to this version, including those read in later steps.
const SECTIONS: &[&str] = &["notes", "vault", "keys", "theme"];

/// How the app arranges its parts (F-28).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// No panels: bufferline, editor, status line, command line.
    #[default]
    Minimal,
    /// Explorer and side panel open, buffers as tabs.
    Classic,
    /// Only the text column.
    Focus,
}

impl Layout {
    /// Every layout, in the order `<leader>l` cycles through them.
    pub const ALL: [Layout; 3] = [Layout::Minimal, Layout::Classic, Layout::Focus];

    /// The name used in the configuration and in `:layout`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Classic => "classic",
            Self::Focus => "focus",
        }
    }

    /// Parses a layout name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }

    /// The layout after this one.
    pub fn next(self) -> Self {
        match self {
            Self::Minimal => Self::Classic,
            Self::Classic => Self::Focus,
            Self::Focus => Self::Minimal,
        }
    }
}

/// Which renderer the app draws with (docs/design/0002, Q-004).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renderer {
    /// CPU rendering: less memory, faster start. The default.
    #[default]
    TinySkia,
    /// GPU rendering: smoother full-screen scrolling, about 70 MB more memory.
    Wgpu,
}

impl Renderer {
    /// The name in the configuration, which is also what Iced's
    /// `ICED_BACKEND` accepts.
    pub fn name(self) -> &'static str {
        match self {
            Self::TinySkia => "tiny-skia",
            Self::Wgpu => "wgpu",
        }
    }
}

/// What the global configuration says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// `[vault] default`: the vault used when neither `--vault` nor
    /// `PATINOTE_VAULT` is given (F-01). `~/` is expanded.
    pub vault_default: Option<PathBuf>,
    /// `layout = "minimal" | "classic" | "focus"`: the layout of a new
    /// install (F-28). The last layout used is remembered in the state.
    pub layout: Option<Layout>,
    /// `renderer = "tiny-skia" | "wgpu"`.
    pub renderer: Option<Renderer>,
    /// `[editor] insert_escape = "jk"`: keys that leave insert mode (F-06).
    pub insert_escape: Option<String>,
    /// `[editor] relative_numbers = false`: real line numbers on every line
    /// instead of relative ones (F-05). `:set rnu` changes it per vault.
    pub relative_numbers: Option<bool>,
    /// Unknown keys and other problems that did not stop reading.
    pub warnings: Vec<String>,
}

/// Why the configuration could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{} is not valid TOML: {message}", path.display())]
    Parse { path: PathBuf, message: String },
}

/// `<config home>/patinote/config.toml`.
pub fn global_config_path(config_home: &Path) -> PathBuf {
    config_home.join("patinote").join("config.toml")
}

/// Expands a leading `~/` with `$HOME`.
pub fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Reads the global configuration. A missing file is an empty configuration.
pub fn load_global(path: &Path) -> Result<Config, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    parse(&text).map_err(|message| ConfigError::Parse {
        path: path.to_path_buf(),
        message,
    })
}

/// Parses configuration text.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| e.message().to_owned())?;
    let mut config = Config::default();
    for (key, value) in &table {
        match key.as_str() {
            "config_version" => match value.as_integer() {
                Some(v) if v > CONFIG_VERSION => config.warnings.push(format!(
                    "config_version {v} is newer than this Patinote understands ({CONFIG_VERSION})"
                )),
                Some(_) => {}
                None => config
                    .warnings
                    .push("config_version must be a number".into()),
            },
            "vault" => {
                let Some(vault) = value.as_table() else {
                    config.warnings.push("[vault] must be a table".into());
                    continue;
                };
                for (k, v) in vault {
                    match (k.as_str(), v.as_str()) {
                        ("default", Some(p)) => config.vault_default = Some(expand_home(p)),
                        ("default", None) => config
                            .warnings
                            .push("[vault] default must be a path".into()),
                        // Read by later steps (symlinks, attachments folder, …).
                        _ => {}
                    }
                }
            }
            "layout" => match value.as_str().and_then(Layout::from_name) {
                Some(l) => config.layout = Some(l),
                None => config
                    .warnings
                    .push("layout must be \"minimal\", \"classic\" or \"focus\"".into()),
            },
            "editor" => {
                let Some(editor) = value.as_table() else {
                    config.warnings.push("[editor] must be a table".into());
                    continue;
                };
                for (k, v) in editor {
                    if k == "relative_numbers" {
                        match v.as_bool() {
                            Some(b) => config.relative_numbers = Some(b),
                            None => config
                                .warnings
                                .push("[editor] relative_numbers must be true or false".into()),
                        }
                        continue;
                    }
                    match (k.as_str(), v.as_str()) {
                        ("insert_escape", Some(keys)) if keys.chars().count() == 2 => {
                            config.insert_escape = Some(keys.to_owned());
                        }
                        ("insert_escape", _) => config.warnings.push(
                            "[editor] insert_escape must be two characters, like \"jk\"".into(),
                        ),
                        // Read by later steps.
                        _ => {}
                    }
                }
            }
            "renderer" => match value.as_str() {
                Some("tiny-skia") => config.renderer = Some(Renderer::TinySkia),
                Some("wgpu") => config.renderer = Some(Renderer::Wgpu),
                _ => config
                    .warnings
                    .push("renderer must be \"tiny-skia\" or \"wgpu\"".into()),
            },
            s if SECTIONS.contains(&s) => {}
            other => config
                .warnings
                .push(format!("unknown key `{other}` in the configuration")),
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_default_vault_and_warns_about_unknown_keys() {
        let c = parse(
            "config_version = 1\nfoo = 2\n[vault]\ndefault = \"/vaults/main\"\n[editor]\nx = 1\n",
        )
        .unwrap();
        assert_eq!(c.vault_default.as_deref(), Some(Path::new("/vaults/main")));
        assert_eq!(c.warnings, ["unknown key `foo` in the configuration"]);
        let c = parse("config_version = 9\n").unwrap();
        assert_eq!(c.warnings.len(), 1);
        assert!(parse("[vault\n").is_err());
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn reads_the_layout_and_renderer() {
        let c = parse("layout = \"classic\"\nrenderer = \"wgpu\"\n").unwrap();
        assert_eq!(c.layout, Some(Layout::Classic));
        assert_eq!(c.renderer, Some(Renderer::Wgpu));
        assert!(c.warnings.is_empty());
        let c = parse("layout = \"tiled\"\nrenderer = 3\n").unwrap();
        assert_eq!((c.layout, c.renderer), (None, None));
        assert_eq!(c.warnings.len(), 2);
        assert_eq!(Layout::Focus.next(), Layout::Minimal);
        let c = parse("[editor]\nrelative_numbers = false\n").unwrap();
        assert_eq!(c.relative_numbers, Some(false));
        assert_eq!(
            parse("[editor]\nrelative_numbers = \"no\"\n")
                .unwrap()
                .warnings
                .len(),
            1
        );
        let c = parse("[editor]\ninsert_escape = \"jk\"\n").unwrap();
        assert_eq!(c.insert_escape.as_deref(), Some("jk"));
        assert_eq!(
            parse("[editor]\ninsert_escape = \"j\"\n")
                .unwrap()
                .warnings
                .len(),
            1
        );
    }
}
