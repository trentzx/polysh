/// Split a command into words, honouring single and double quotes.
///
/// Backslashes are *not* escape characters, because they're path separators
/// on Windows. Returns `None` if a quote is left open.
pub fn tokenize(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;

    for ch in line.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    cur.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    in_token = true;
                }
                c if c.is_whitespace() => {
                    if in_token {
                        tokens.push(std::mem::take(&mut cur));
                        in_token = false;
                    }
                }
                c => {
                    cur.push(c);
                    in_token = true;
                }
            },
        }
    }

    if quote.is_some() {
        return None;
    }
    if in_token {
        tokens.push(cur);
    }
    Some(tokens)
}

/// One part of a command line: a simple command, or the operator between two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Command(String),
    Op(&'static str),
}

/// Split a line at unquoted `|`, `||`, `&&`, `&` and `;`.
///
/// Redirections (`> out.txt`, `2>&1`) stay inside their command. Returns
/// `None` for things polysh can't safely rewrite: backticks, `$( … )`, and
/// unbalanced quotes.
pub fn split_ops(line: &str) -> Option<Vec<Piece>> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '`' => return None,
            '$' if next == Some('(') => return None,
            '|' => {
                flush(&mut out, &mut cur);
                if next == Some('|') {
                    out.push(Piece::Op("||"));
                    i += 1;
                } else {
                    out.push(Piece::Op("|"));
                }
            }
            '&' if next == Some('&') => {
                flush(&mut out, &mut cur);
                out.push(Piece::Op("&&"));
                i += 1;
            }
            // `2>&1`, `>&2`: part of a redirection, not an operator.
            '&' if cur.ends_with('>') => cur.push(c),
            '&' => {
                flush(&mut out, &mut cur);
                out.push(Piece::Op("&"));
            }
            ';' => {
                flush(&mut out, &mut cur);
                out.push(Piece::Op(";"));
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    if quote.is_some() {
        return None;
    }
    flush(&mut out, &mut cur);
    Some(out)
}

fn flush(out: &mut Vec<Piece>, cur: &mut String) {
    let t = cur.trim();
    if !t.is_empty() {
        out.push(Piece::Command(t.to_string()));
    }
    cur.clear();
}

/// Split `ls -la > out.txt` into (`ls -la`, `> out.txt`).
pub fn split_redirect(cmd: &str) -> (&str, &str) {
    let mut quote: Option<char> = None;
    let bytes = cmd.as_bytes();
    for (i, c) in cmd.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '>' | '<' => {
                    // Include a file-descriptor digit: `cmd 2>err.txt`.
                    let start = if i >= 1
                        && bytes[i - 1].is_ascii_digit()
                        && (i == 1 || bytes[i - 2].is_ascii_whitespace())
                    {
                        i - 1
                    } else {
                        i
                    };
                    return (cmd[..start].trim_end(), &cmd[start..]);
                }
                _ => {}
            },
        }
    }
    (cmd, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_unquotes() {
        assert_eq!(
            tokenize(r#"cp "my file.txt" 'other dir' C:\x"#).unwrap(),
            vec!["cp", "my file.txt", "other dir", r"C:\x"]
        );
    }

    #[test]
    fn unbalanced_quote_is_none() {
        assert!(tokenize("echo \"oops").is_none());
        assert!(split_ops("echo \"oops").is_none());
    }

    #[test]
    fn splits_on_operators_outside_quotes() {
        use Piece::*;
        assert_eq!(
            split_ops("ls -la | grep \"a|b\" && echo ok; cls").unwrap(),
            vec![
                Command("ls -la".into()),
                Op("|"),
                Command("grep \"a|b\"".into()),
                Op("&&"),
                Command("echo ok".into()),
                Op(";"),
                Command("cls".into()),
            ]
        );
        assert_eq!(
            split_ops("make 2>&1 || exit").unwrap(),
            vec![
                Command("make 2>&1".into()),
                Op("||"),
                Command("exit".into())
            ]
        );
        assert!(split_ops("echo $(whoami)").is_none());
    }

    #[test]
    fn splits_redirection() {
        assert_eq!(split_redirect("ls -la > out.txt"), ("ls -la", "> out.txt"));
        assert_eq!(
            split_redirect("cargo build 2>/dev/null"),
            ("cargo build", "2>/dev/null")
        );
        assert_eq!(split_redirect("echo \"a > b\""), ("echo \"a > b\"", ""));
        assert_eq!(split_redirect("sort < in.txt"), ("sort", "< in.txt"));
    }
}
