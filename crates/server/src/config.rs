//! Server configuration from environment variables. Every variable has a safe default for local runs;
//! the list with defaults is in `.env.example`.

use std::fmt;
use std::net::SocketAddr;

use thiserror::Error;

use crate::client::IpRange;

/// Default address: loopback only, so a dev server is never exposed by accident.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7700";
/// Where uploaded files are kept when `PROSTOR_FILES_DIR` is not set.
pub const DEFAULT_FILES_DIR: &str = "data/files";
/// The largest upload any setting may allow when `PROSTOR_UPLOAD_CEILING_BYTES` is not set: 1 GiB.
pub const DEFAULT_UPLOAD_CEILING: u64 = 1024 * 1024 * 1024;
/// Where avatars are imported from when `PROSTOR_GRAVATAR_URL` is not set.
pub const DEFAULT_GRAVATAR_URL: &str = "https://gravatar.com/avatar/";

/// The dev database of `make db-up` (deploy/compose.dev.yaml).
pub const DEFAULT_DATABASE_URL: &str = "postgres://prostor:prostor@127.0.0.1:5433/prostor";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable lines for development.
    Text,
    /// One JSON object per line for log collectors.
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `PROSTOR_LISTEN` — address and port to listen on.
    pub listen: SocketAddr,
    /// `PROSTOR_LOG_FORMAT` — `text` (default) or `json`. The level filter comes from `RUST_LOG`.
    pub log_format: LogFormat,
    pub database: DatabaseConfig,
    /// `PROSTOR_JOB_WORKERS` — background jobs that run at the same time on this node (default 4).
    pub job_workers: usize,
    /// `PROSTOR_TRUSTED_PROXIES` — comma-separated addresses or ranges (`10.0.0.0/8`) of reverse
    /// proxies in front of the server; behind them the client address comes from
    /// `X-Forwarded-For`. Default: none.
    pub trusted_proxies: Vec<IpRange>,
    /// `PROSTOR_SETUP_CODE` — the code the first-run setup asks for, instead of a random one
    /// printed in the log; for automated installations.
    pub setup_code: Option<String>,
    /// Outgoing email; `None` turns account emails off (invitations, password resets).
    pub mail: Option<MailConfig>,
    /// `PROSTOR_SECRET_KEY` — the base64 of 32 bytes that seals secret settings (NTF-004).
    /// `None`: the mail screen refuses to save secrets and says so.
    pub secret_key: Option<[u8; 32]>,
    pub files: FilesConfig,
}

/// Uploaded files (D-033).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesConfig {
    /// `PROSTOR_FILES_DIR` — the directory of uploaded files (default `data/files`). Every node
    /// must see the same directory; backups must include it.
    pub dir: String,
    /// `PROSTOR_UPLOAD_CEILING_BYTES` — the largest upload, whatever the administrators set
    /// (default 1 GiB). The limit in force is the smaller of this and the `upload_max_bytes`
    /// setting (ADM-004).
    pub upload_ceiling: u64,
    /// `PROSTOR_GRAVATAR_URL` — where avatars are imported from (default
    /// `https://gravatar.com/avatar/`); empty turns the import off (ACC-017).
    pub gravatar_url: Option<String>,
}

/// The outgoing mail server, until the admin settings of NTF-004 (D-026).
#[derive(Clone, PartialEq, Eq)]
pub struct MailConfig {
    /// `PROSTOR_SMTP_URL` — `smtp://user:password@host:587` (STARTTLS when the server offers
    /// it; `?tls=required` insists), `smtps://…:465` (TLS from the start), or
    /// `smtp://127.0.0.1:1025` for Mailpit in development.
    pub smtp_url: String,
    /// `PROSTOR_MAIL_FROM` — the sender, such as `Prostor <tracker@example.com>`.
    pub from: String,
}

impl fmt::Debug for MailConfig {
    // Keeps the SMTP password out of logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailConfig")
            .field("smtp_url", &redact_password(&self.smtp_url))
            .field("from", &self.from)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    /// `DATABASE_URL` — `postgres://user:password@host:port/database`; the same variable
    /// sqlx-cli and the compile-time query checks use.
    pub url: String,
    /// `PROSTOR_DB_MAX_CONNECTIONS` — pool size of this node (default 20, at least job workers + 2).
    /// All nodes together must stay below the server's `max_connections`.
    pub max_connections: u32,
}

impl fmt::Debug for DatabaseConfig {
    // Keeps the password out of logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &redact_password(&self.url))
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

/// The URL with every password (`user:password@` and `?password=`, `?sslpassword=`) replaced by
/// `***`, for logs.
pub fn redact_password(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return "<invalid URL>".to_owned();
    };
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("***"));
    }
    if parsed.query().is_some() {
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(key, value)| {
                let value = if key.to_ascii_lowercase().contains("password") {
                    "***".to_owned()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        parsed.query_pairs_mut().clear().extend_pairs(pairs);
    }
    parsed.to_string()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{name} must be an address like 127.0.0.1:7700, got {value:?}")]
    BadAddress { name: &'static str, value: String },
    #[error("{name} must be `text` or `json`, got {value:?}")]
    BadLogFormat { name: &'static str, value: String },
    #[error("{name} must be a postgres:// URL")]
    BadDatabaseUrl { name: &'static str },
    #[error("{name} must be a whole number from {min} to {max}, got {value:?}")]
    BadNumber {
        name: &'static str,
        value: String,
        min: u32,
        max: u32,
    },
    #[error("PROSTOR_TRUSTED_PROXIES: {0}")]
    BadProxy(String),
    #[error("PROSTOR_SETUP_CODE must have at least 8 letters or digits")]
    BadSetupCode,
    #[error("PROSTOR_FILES_DIR must not be empty")]
    BadFilesDir,
    #[error("{name} must be a whole number of bytes from {min} to {max}, got {value:?}")]
    BadBytes {
        name: &'static str,
        value: String,
        min: u64,
        max: u64,
    },
    #[error("PROSTOR_GRAVATAR_URL must be an http:// or https:// URL, or empty")]
    BadGravatarUrl,
    #[error("PROSTOR_SMTP_URL must be an smtp:// or smtps:// URL")]
    BadSmtpUrl,
    #[error(
        "PROSTOR_MAIL_FROM must be set with PROSTOR_SMTP_URL, like `Prostor <tracker@example.com>`"
    )]
    MissingMailFrom,
    #[error("PROSTOR_SECRET_KEY must be the base64 of 32 bytes: {0}")]
    BadSecretKey(String),
    #[error(
        "PROSTOR_DB_MAX_CONNECTIONS ({connections}) must be at least PROSTOR_JOB_WORKERS ({workers}) \
         + 2: one connection listens for notifications, and requests need at least one more"
    )]
    PoolTooSmall { connections: u32, workers: u32 },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads configuration through `lookup`, so tests don't touch the process environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let listen_raw = lookup("PROSTOR_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let listen = listen_raw.parse().map_err(|_| ConfigError::BadAddress {
            name: "PROSTOR_LISTEN",
            value: listen_raw,
        })?;

        let log_format = match lookup("PROSTOR_LOG_FORMAT").as_deref() {
            None | Some("text") => LogFormat::Text,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(ConfigError::BadLogFormat {
                    name: "PROSTOR_LOG_FORMAT",
                    value: other.to_owned(),
                });
            }
        };

        let url = lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());
        let postgres = url.starts_with("postgres://") || url.starts_with("postgresql://");
        if !postgres || prostor_store::connect_options(&url).is_err() {
            return Err(ConfigError::BadDatabaseUrl {
                name: "DATABASE_URL",
            });
        }
        let max_connections = number(&lookup, "PROSTOR_DB_MAX_CONNECTIONS", 20, 4, 1000)?;
        let job_workers = number(&lookup, "PROSTOR_JOB_WORKERS", 4, 1, 64)?;
        if max_connections < job_workers + 2 {
            return Err(ConfigError::PoolTooSmall {
                connections: max_connections,
                workers: job_workers,
            });
        }

        let trusted_proxies = lookup("PROSTOR_TRUSTED_PROXIES")
            .unwrap_or_default()
            .split(',')
            .filter(|item| !item.trim().is_empty())
            .map(str::parse)
            .collect::<Result<Vec<IpRange>, String>>()
            .map_err(ConfigError::BadProxy)?;

        let setup_code = lookup("PROSTOR_SETUP_CODE").filter(|code| !code.trim().is_empty());
        if setup_code
            .as_deref()
            .is_some_and(|code| prostor_domain::auth::codes::normalize(code).len() < 8)
        {
            return Err(ConfigError::BadSetupCode);
        }

        let mail = match lookup("PROSTOR_SMTP_URL").filter(|url| !url.trim().is_empty()) {
            None => None,
            Some(smtp_url) => {
                let scheme_ok = url::Url::parse(smtp_url.trim())
                    .is_ok_and(|url| matches!(url.scheme(), "smtp" | "smtps"));
                if !scheme_ok {
                    return Err(ConfigError::BadSmtpUrl);
                }
                let from = lookup("PROSTOR_MAIL_FROM")
                    .filter(|from| !from.trim().is_empty())
                    .ok_or(ConfigError::MissingMailFrom)?;
                Some(MailConfig {
                    smtp_url: smtp_url.trim().to_owned(),
                    from: from.trim().to_owned(),
                })
            }
        };

        let files = FilesConfig {
            dir: match lookup("PROSTOR_FILES_DIR") {
                None => DEFAULT_FILES_DIR.to_owned(),
                Some(dir) if dir.trim().is_empty() => return Err(ConfigError::BadFilesDir),
                Some(dir) => dir.trim().to_owned(),
            },
            upload_ceiling: bytes(
                &lookup,
                "PROSTOR_UPLOAD_CEILING_BYTES",
                DEFAULT_UPLOAD_CEILING,
                1,
                // 64 GiB: far beyond anything a tracker takes, still a sane number.
                64 * 1024 * 1024 * 1024,
            )?,
            gravatar_url: match lookup("PROSTOR_GRAVATAR_URL") {
                None => Some(DEFAULT_GRAVATAR_URL.to_owned()),
                Some(url) if url.trim().is_empty() => None,
                Some(url) => {
                    let ok = url::Url::parse(url.trim())
                        .is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
                    if !ok {
                        return Err(ConfigError::BadGravatarUrl);
                    }
                    Some(url.trim().to_owned())
                }
            },
        };

        let secret_key = match lookup("PROSTOR_SECRET_KEY").filter(|key| !key.trim().is_empty()) {
            None => None,
            Some(raw) => Some(
                prostor_domain::secret::Key::from_base64(&raw)
                    .map_err(ConfigError::BadSecretKey)?
                    .into_bytes(),
            ),
        };
        Ok(Self {
            listen,
            log_format,
            database: DatabaseConfig {
                url,
                max_connections,
            },
            job_workers: usize::try_from(job_workers).unwrap_or(1),
            trusted_proxies,
            setup_code,
            mail,
            secret_key,
            files,
        })
    }
}

fn bytes(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, ConfigError> {
    let Some(raw) = lookup(name) else {
        return Ok(default);
    };
    match raw.trim().parse::<u64>() {
        Ok(value) if (min..=max).contains(&value) => Ok(value),
        _ => Err(ConfigError::BadBytes {
            name,
            value: raw,
            min,
            max,
        }),
    }
}

fn number(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: u32,
    min: u32,
    max: u32,
) -> Result<u32, ConfigError> {
    let Some(raw) = lookup(name) else {
        return Ok(default);
    };
    match raw.trim().parse::<u32>() {
        Ok(value) if (min..=max).contains(&value) => Ok(value),
        _ => Err(ConfigError::BadNumber {
            name,
            value: raw,
            min,
            max,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn defaults_when_nothing_is_set() {
        let config = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(config.listen, DEFAULT_LISTEN.parse().unwrap());
        assert_eq!(config.log_format, LogFormat::Text);
        assert_eq!(config.database.url, DEFAULT_DATABASE_URL);
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.job_workers, 4);
        assert!(config.trusted_proxies.is_empty());
        assert_eq!(config.setup_code, None);
        assert_eq!(config.mail, None);
        assert_eq!(config.files.dir, DEFAULT_FILES_DIR);
        assert_eq!(config.files.upload_ceiling, DEFAULT_UPLOAD_CEILING);
        assert_eq!(
            config.files.gravatar_url.as_deref(),
            Some(DEFAULT_GRAVATAR_URL)
        );
    }

    #[test]
    fn adm_004_reads_the_files_settings() {
        let config = Config::from_lookup(lookup(&[
            ("PROSTOR_FILES_DIR", "/srv/prostor/files"),
            ("PROSTOR_UPLOAD_CEILING_BYTES", "5242880"),
            ("PROSTOR_GRAVATAR_URL", ""),
        ]))
        .unwrap();
        assert_eq!(config.files.dir, "/srv/prostor/files");
        assert_eq!(config.files.upload_ceiling, 5_242_880);
        assert_eq!(config.files.gravatar_url, None);
        for (name, value) in [
            ("PROSTOR_FILES_DIR", " "),
            ("PROSTOR_UPLOAD_CEILING_BYTES", "0"),
            ("PROSTOR_UPLOAD_CEILING_BYTES", "lots"),
            ("PROSTOR_GRAVATAR_URL", "ftp://example.com/"),
        ] {
            assert!(
                Config::from_lookup(lookup(&[(name, value)])).is_err(),
                "{name}={value}"
            );
        }
    }

    #[test]
    fn reads_the_mail_server() {
        let config = Config::from_lookup(lookup(&[
            (
                "PROSTOR_SMTP_URL",
                "smtps://bot:secret@mail.example.com:465",
            ),
            ("PROSTOR_MAIL_FROM", "Prostor <tracker@example.com>"),
        ]))
        .unwrap();
        let mail = config.mail.unwrap();
        assert_eq!(mail.from, "Prostor <tracker@example.com>");
        assert!(!format!("{mail:?}").contains("secret"));
        assert_eq!(
            Config::from_lookup(lookup(&[("PROSTOR_SMTP_URL", "http://mail")])).unwrap_err(),
            ConfigError::BadSmtpUrl
        );
        assert_eq!(
            Config::from_lookup(lookup(&[("PROSTOR_SMTP_URL", "smtp://127.0.0.1:1025")]))
                .unwrap_err(),
            ConfigError::MissingMailFrom
        );
    }

    #[test]
    fn reads_values() {
        let config = Config::from_lookup(lookup(&[
            ("PROSTOR_LISTEN", "0.0.0.0:9000"),
            ("PROSTOR_LOG_FORMAT", "json"),
            (
                "DATABASE_URL",
                "postgres://app:secret@db.internal:5432/tracker",
            ),
            ("PROSTOR_DB_MAX_CONNECTIONS", "50"),
            ("PROSTOR_JOB_WORKERS", "8"),
            ("PROSTOR_TRUSTED_PROXIES", "127.0.0.1, 172.16.0.0/12"),
            ("PROSTOR_SETUP_CODE", "abcd-efgh-2345"),
        ]))
        .unwrap();
        assert_eq!(
            config
                .trusted_proxies
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["127.0.0.1/32", "172.16.0.0/12"]
        );
        assert_eq!(config.setup_code.as_deref(), Some("abcd-efgh-2345"));
        assert_eq!(config.listen, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(config.log_format, LogFormat::Json);
        assert_eq!(
            config.database.url,
            "postgres://app:secret@db.internal:5432/tracker"
        );
        assert_eq!(config.database.max_connections, 50);
        assert_eq!(config.job_workers, 8);
    }

    #[test]
    fn rejects_bad_values() {
        assert!(matches!(
            Config::from_lookup(lookup(&[("PROSTOR_LISTEN", "localhost")])),
            Err(ConfigError::BadAddress { .. })
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[("PROSTOR_LOG_FORMAT", "yaml")])),
            Err(ConfigError::BadLogFormat { .. })
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[("DATABASE_URL", "mysql://localhost/db")])),
            Err(ConfigError::BadDatabaseUrl { .. })
        ));
        for bad in ["0", "many", "100000"] {
            assert!(matches!(
                Config::from_lookup(lookup(&[("PROSTOR_JOB_WORKERS", bad)])),
                Err(ConfigError::BadNumber { .. })
            ));
        }
        assert!(matches!(
            Config::from_lookup(lookup(&[("PROSTOR_TRUSTED_PROXIES", "10.0.0.0/40")])),
            Err(ConfigError::BadProxy(_))
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[("PROSTOR_SETUP_CODE", "short")])),
            Err(ConfigError::BadSetupCode)
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[
                ("PROSTOR_DB_MAX_CONNECTIONS", "10"),
                ("PROSTOR_JOB_WORKERS", "9"),
            ])),
            Err(ConfigError::PoolTooSmall { .. })
        ));
    }

    #[test]
    fn redacts_passwords_in_the_query_string_too() {
        let redacted =
            redact_password("postgres://app@db/x?sslmode=require&password=secret&sslpassword=key");
        assert!(
            !redacted.contains("secret") && !redacted.contains("key"),
            "{redacted}"
        );
        assert!(redacted.contains("sslmode=require"), "{redacted}");
        assert_eq!(redact_password("not a url"), "<invalid URL>");
    }

    #[test]
    fn debug_output_hides_the_database_password() {
        let config = Config::from_lookup(lookup(&[(
            "DATABASE_URL",
            "postgres://app:hunter2@db.internal:5432/tracker",
        )]))
        .unwrap();
        let printed = format!("{config:?}");
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains("app:***@db.internal"), "{printed}");
        // The key's field name appears; its bytes never do.
        assert!(printed.contains("secret_key: None"), "{printed}");
    }
}
