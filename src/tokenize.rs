/// Split a command line into words, honouring single and double quotes.
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

/// True if the line uses shell operators (pipes, redirection, chaining)
/// outside of quotes. polysh passes such lines through untranslated for now.
pub fn has_operators(line: &str) -> bool {
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '|' | '>' | '<' | '&' | ';' | '`' => return true,
                _ => {}
            },
        }
    }
    false
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
    }

    #[test]
    fn operators_outside_quotes_only() {
        assert!(has_operators("ls | grep x"));
        assert!(!has_operators("grep \"a|b\" file"));
    }
}
