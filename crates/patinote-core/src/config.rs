//! The global configuration file, `$XDG_CONFIG_HOME/patinote/config.toml`
//! (PRD "Gerbang 2: dampak arsitektur", point 3).
//!
//! Unknown keys give warnings, not errors, so an older Patinote can read a
//! newer file. Step 1.7 reads only `[vault] default`; the other sections are
//! accepted and read by later steps.

use std::path::{Path, PathBuf};

/// The configuration format this version writes.
pub const CONFIG_VERSION: i64 = 1;

/// Sections known to this version, including those read in later steps.
const SECTIONS: &[&str] = &["editor", "layout", "notes", "vault", "keys", "theme"];

/// What the global configuration says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// `[vault] default`: the vault used when neither `--vault` nor
    /// `PATINOTE_VAULT` is given (F-01). `~/` is expanded.
    pub vault_default: Option<PathBuf>,
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
}
