//! Parsing of the `Exec` key according to the Desktop Entry Specification
//! ("The Exec key" section). The result is an argv vector that is handed to
//! the process API directly; nothing here ever goes through a shell.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    Empty,
    UnterminatedQuote,
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::Empty => write!(f, "Exec line is empty"),
            ExecError::UnterminatedQuote => write!(f, "Exec line has an unterminated quote"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Values substituted for field codes when launching without files/URIs.
#[derive(Debug, Default, Clone)]
pub struct FieldContext<'a> {
    pub name: &'a str,
    pub icon: Option<&'a str>,
    pub desktop_file: Option<&'a str>,
}

/// Splits an `Exec` value (already unescaped at the key-file level) into
/// arguments, honouring the spec's double-quote rules.
pub fn split_exec(exec: &str) -> Result<Vec<String>, ExecError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut in_quotes = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => in_quotes = false,
                '\\' => match chars.peek() {
                    Some(&next @ ('"' | '`' | '$' | '\\')) => {
                        current.push(next);
                        chars.next();
                    }
                    _ => current.push('\\'),
                },
                _ => current.push(c),
            }
            continue;
        }
        match c {
            ' ' | '\t' | '\n' => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            '"' => {
                in_quotes = true;
                in_arg = true;
            }
            // Not allowed unquoted by the spec, but common in the wild; treat
            // a backslash as escaping the next character like a shell would.
            '\\' => {
                in_arg = true;
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ => {
                in_arg = true;
                current.push(c);
            }
        }
    }

    if in_quotes {
        return Err(ExecError::UnterminatedQuote);
    }
    if in_arg {
        args.push(current);
    }
    if args.is_empty() {
        return Err(ExecError::Empty);
    }
    Ok(args)
}

/// Expands field codes for a launch without files. `%f %F %u %U` and the
/// deprecated codes are dropped, `%i %c %k %%` are substituted.
pub fn expand_field_codes(args: Vec<String>, ctx: &FieldContext<'_>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        match arg.as_str() {
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
            "%i" => {
                if let Some(icon) = ctx.icon.filter(|i| !i.is_empty()) {
                    out.push("--icon".to_owned());
                    out.push(icon.to_owned());
                }
                continue;
            }
            _ => {}
        }
        let mut expanded = String::with_capacity(arg.len());
        let mut chars = arg.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                expanded.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('c') => expanded.push_str(ctx.name),
                Some('k') => expanded.push_str(ctx.desktop_file.unwrap_or("")),
                Some('i') => expanded.push_str(ctx.icon.unwrap_or("")),
                // File/URL codes embedded in a larger argument, deprecated
                // and unknown codes: removed.
                Some(_) | None => {}
            }
        }
        // An argument that consisted only of removed codes vanishes.
        if expanded.is_empty() && !arg.is_empty() && arg.chars().all(|c| c == '%' || c.is_ascii_alphabetic()) {
            continue;
        }
        out.push(expanded);
    }
    out
}

/// Convenience: split + expand.
pub fn exec_to_argv(exec: &str, ctx: &FieldContext<'_>) -> Result<Vec<String>, ExecError> {
    let argv = expand_field_codes(split_exec(exec)?, ctx);
    if argv.is_empty() {
        return Err(ExecError::Empty);
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FieldContext<'static> {
        FieldContext {
            name: "Fire Fox",
            icon: Some("firefox"),
            desktop_file: Some("/usr/share/applications/firefox.desktop"),
        }
    }

    #[test]
    fn splits_plain_arguments() {
        assert_eq!(split_exec("firefox --new-window").unwrap(), vec!["firefox", "--new-window"]);
    }

    #[test]
    fn collapses_repeated_whitespace() {
        assert_eq!(split_exec("  a \t b  ").unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn honours_double_quotes_and_escapes() {
        let args = split_exec(r#""/opt/My App/bin" "say \"hi\"" "a\\b" "\$HOME" "\`x\`""#).unwrap();
        assert_eq!(args, vec!["/opt/My App/bin", "say \"hi\"", "a\\b", "$HOME", "`x`"]);
    }

    #[test]
    fn keeps_unknown_backslash_inside_quotes() {
        assert_eq!(split_exec(r#""a\nb""#).unwrap(), vec!["a\\nb"]);
    }

    #[test]
    fn empty_quoted_argument_is_kept() {
        assert_eq!(split_exec(r#"app "" x"#).unwrap(), vec!["app", "", "x"]);
    }

    #[test]
    fn shell_syntax_is_not_interpreted() {
        assert_eq!(split_exec("sh -c echo;rm").unwrap(), vec!["sh", "-c", "echo;rm"]);
        assert_eq!(split_exec("app $(evil) `x` | y").unwrap(), vec!["app", "$(evil)", "`x`", "|", "y"]);
    }

    #[test]
    fn rejects_unterminated_quote_and_empty() {
        assert_eq!(split_exec(r#"app "oops"#), Err(ExecError::UnterminatedQuote));
        assert_eq!(split_exec("   "), Err(ExecError::Empty));
    }

    #[test]
    fn removes_file_field_codes() {
        let argv = exec_to_argv("firefox %u", &ctx()).unwrap();
        assert_eq!(argv, vec!["firefox"]);
        let argv = exec_to_argv("code --unity-launch %F", &ctx()).unwrap();
        assert_eq!(argv, vec!["code", "--unity-launch"]);
    }

    #[test]
    fn expands_icon_name_and_location() {
        let argv = exec_to_argv("app %i --title=%c --from %k", &ctx()).unwrap();
        assert_eq!(
            argv,
            vec![
                "app",
                "--icon",
                "firefox",
                "--title=Fire Fox",
                "--from",
                "/usr/share/applications/firefox.desktop"
            ]
        );
    }

    #[test]
    fn icon_code_without_icon_disappears() {
        let c = FieldContext {
            name: "x",
            icon: None,
            desktop_file: None,
        };
        assert_eq!(exec_to_argv("app %i", &c).unwrap(), vec!["app"]);
    }

    #[test]
    fn percent_escape_and_embedded_codes() {
        assert_eq!(exec_to_argv("printf 100%% --file=%f", &ctx()).unwrap(), vec!["printf", "100%", "--file="]);
    }

    #[test]
    fn only_field_codes_is_an_error() {
        assert_eq!(exec_to_argv("%U", &ctx()), Err(ExecError::Empty));
    }
}
