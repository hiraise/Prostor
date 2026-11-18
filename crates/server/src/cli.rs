//! The command line of the `prostor` binary: running the server, and administrator recovery
//! for whoever runs the server (ADM-051, AUTH-073).

use prostor_service::recovery::Target;

/// What the binary was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    RecoverAdmin(Target),
    Help,
    Version,
}

pub const USAGE: &str = "\
Prostor — an issue tracker

Usage:
  prostor [serve]
      Run the server. Settings come from environment variables (see .env.example).

  prostor recover-admin [LOGIN]
      Restore administrator access when nobody can sign in. Gives the account a new random
      password (printed once), enables it, makes it a system administrator and ends its
      sessions. LOGIN is a username or email; without it, the administrator made by the setup.

  prostor recover-admin --new USERNAME EMAIL [FULL NAME]
      Create a new system administrator with a random password.

  prostor help | version

Recovery uses the same DATABASE_URL as the server and is recorded in the audit log.
";

/// Reads the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<String> = args.into_iter().collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["serve"] => Ok(Command::Serve),
        ["help" | "--help" | "-h"] => Ok(Command::Help),
        ["version" | "--version" | "-V"] => Ok(Command::Version),
        ["recover-admin"] => Ok(Command::RecoverAdmin(Target::BuiltinAdmin)),
        ["recover-admin", "--new", username, email, name @ ..] => {
            let full_name = (!name.is_empty()).then(|| name.join(" "));
            Ok(Command::RecoverAdmin(Target::New {
                username: (*username).to_owned(),
                email: (*email).to_owned(),
                full_name,
            }))
        }
        ["recover-admin", "--new", ..] => {
            Err("recover-admin --new needs a username and an email".to_owned())
        }
        ["recover-admin", login] if !login.starts_with('-') => {
            Ok(Command::RecoverAdmin(Target::Login((*login).to_owned())))
        }
        _ => Err(format!("unknown command: {}", words.join(" "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Command, String> {
        parse(words.iter().map(|word| (*word).to_owned()))
    }

    #[test]
    fn adm_051_commands() {
        assert_eq!(parse_words(&[]), Ok(Command::Serve));
        assert_eq!(parse_words(&["serve"]), Ok(Command::Serve));
        assert_eq!(parse_words(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_words(&["version"]), Ok(Command::Version));
        assert_eq!(
            parse_words(&["recover-admin"]),
            Ok(Command::RecoverAdmin(Target::BuiltinAdmin))
        );
        assert_eq!(
            parse_words(&["recover-admin", "anna@example.com"]),
            Ok(Command::RecoverAdmin(Target::Login(
                "anna@example.com".to_owned()
            )))
        );
        assert_eq!(
            parse_words(&[
                "recover-admin",
                "--new",
                "ops",
                "ops@example.com",
                "Ops",
                "Team"
            ]),
            Ok(Command::RecoverAdmin(Target::New {
                username: "ops".to_owned(),
                email: "ops@example.com".to_owned(),
                full_name: Some("Ops Team".to_owned()),
            }))
        );
        assert!(parse_words(&["recover-admin", "--new", "ops"]).is_err());
        assert!(parse_words(&["recover-admin", "--force"]).is_err());
        assert!(parse_words(&["migrate"]).is_err());
    }
}
