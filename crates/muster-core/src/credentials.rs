//! Credentials in text from outside Muster, hidden before Muster shows the
//! text or hands it to an agent. A ticket's description can carry a secret
//! someone pasted in: a `curl` command with its `Authorization` header, an
//! API key, a token. Muster must not be what puts it on a screen or sends
//! it on to an agent and its model provider (`PRD-06`, `PRD-07`); the
//! ticket itself, in its tracker, is left as it is.
//!
//! What is hidden: private keys; JSON Web Tokens; what follows `Bearer` or
//! `Basic`; well-known token forms (GitHub, GitLab, Slack, Anthropic,
//! OpenAI, AWS, Google, Linear, Atlassian, npm, PyPI); the values of keys
//! that name a secret (`api_key`, `x-api-key`, `token`, `password`,
//! `client_secret`, `cookie`, …) written `key: value` or `key=value`; and
//! the password in a web address. It errs towards hiding: a value is cheap
//! to look up in the tracker, a leaked credential is not.
//!
//! Patterns are matched on ASCII, so every cut falls between characters.
//! The work is in step with the text's length times the patterns', and
//! never fails.

/// What a hidden credential reads as.
pub const HIDDEN: &str = "[hidden by Muster]";

/// `text` with its credentials hidden, and how many were.
pub fn hide(text: &str) -> (String, usize) {
    let mut found = Vec::new();
    private_keys(text, &mut found);
    web_tokens(text, &mut found);
    known_tokens(text, &mut found);
    schemes(text, &mut found);
    pairs(text, &mut found);
    url_passwords(text, &mut found);
    if found.is_empty() {
        return (text.to_owned(), 0);
    }
    found.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in found {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in &merged {
        out.push_str(&text[at..*start]);
        out.push_str(HIDDEN);
        at = *end;
    }
    out.push_str(&text[at..]);
    (out, merged.len())
}

/// A character a token is made of.
fn tokenish(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-_.~+/=".contains(&byte)
}

/// A character of a base64url segment, as in a JSON Web Token.
fn base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// A character a name is made of: a key's, a word's.
fn wordish(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Where the run of `class` from `at` ends.
fn run(bytes: &[u8], at: usize, class: impl Fn(u8) -> bool) -> usize {
    let mut end = at;
    while end < bytes.len() && class(bytes[end]) {
        end += 1;
    }
    end
}

/// Every place `needle` (lowercase ASCII) is in `text`, ignoring case.
fn find_all(text: &str, needle: &str) -> Vec<usize> {
    let (haystack, needle) = (text.as_bytes(), needle.as_bytes());
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    (0..=haystack.len() - needle.len())
        .filter(|at| haystack[*at..*at + needle.len()].eq_ignore_ascii_case(needle))
        .collect()
}

/// `-----BEGIN … PRIVATE KEY-----` to its end, or to the text's end.
fn private_keys(text: &str, found: &mut Vec<(usize, usize)>) {
    for start in find_all(text, "-----begin") {
        let line_end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        if !text[start..line_end]
            .to_ascii_lowercase()
            .contains("private key")
        {
            continue;
        }
        let end = find_all(&text[line_end..], "-----end")
            .first()
            .map_or(text.len(), |at| {
                let close = line_end + at;
                text[close + 8..]
                    .find("-----")
                    .map_or(text.len(), |dashes| close + 8 + dashes + 5)
            });
        found.push((start, end));
    }
}

/// `eyJ….….…`: a JSON Web Token's three segments.
fn web_tokens(text: &str, found: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    for start in find_all(text, "eyj") {
        if &bytes[start..start + 3] != b"eyJ" || (start > 0 && base64url(bytes[start - 1])) {
            continue;
        }
        let first = run(bytes, start, base64url);
        if first - start < 10 || bytes.get(first) != Some(&b'.') {
            continue;
        }
        let second = run(bytes, first + 1, base64url);
        if second - first - 1 < 10 || bytes.get(second) != Some(&b'.') {
            continue;
        }
        let third = run(bytes, second + 1, base64url);
        found.push((start, third));
    }
}

/// Token forms known by their prefix, with how long what follows must at
/// least be.
const KNOWN: &[(&str, usize)] = &[
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("ghr_", 20),
    ("github_pat_", 20),
    ("glpat-", 16),
    ("xoxb-", 10),
    ("xoxp-", 10),
    ("xoxa-", 10),
    ("xoxs-", 10),
    ("xapp-", 10),
    ("sk-ant-", 20),
    ("sk-proj-", 20),
    ("sk-", 32),
    ("akia", 16),
    ("asia", 16),
    ("aiza", 30),
    ("lin_api_", 20),
    ("lin_oauth_", 20),
    ("atatt", 20),
    ("npm_", 30),
    ("pypi-", 30),
];

fn known_tokens(text: &str, found: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    for (prefix, least) in KNOWN {
        for start in find_all(text, prefix) {
            if start > 0 && wordish(bytes[start - 1]) {
                continue;
            }
            let written = &text[start..start + prefix.len()];
            // AWS's and Google's keys are written in their own case only.
            let cased = match *prefix {
                "akia" | "asia" => written == prefix.to_ascii_uppercase(),
                "aiza" => written == "AIza",
                "atatt" => written == "ATATT",
                _ => true,
            };
            let end = run(bytes, start + prefix.len(), base64url);
            if cased && end - start - prefix.len() >= *least {
                found.push((start, end));
            }
        }
    }
}

/// What follows `Bearer` or `Basic`, the line's end and spaces between.
fn schemes(text: &str, found: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    for (scheme, least) in [("bearer", 12), ("basic", 8)] {
        for start in find_all(text, scheme) {
            let after = start + scheme.len();
            if (start > 0 && wordish(bytes[start - 1]))
                || bytes.get(after).is_some_and(|b| wordish(*b))
            {
                continue;
            }
            let value = after + run(&bytes[after..], 0, |b| b.is_ascii_whitespace());
            if value == after {
                continue;
            }
            let end = run(bytes, value, tokenish);
            if end - value >= least {
                found.push((value, end));
            }
        }
    }
}

/// Keys whose values are secrets, and how long a value must at least be.
const SECRET_KEYS: &[(&str, usize)] = &[
    ("x-api-key", 8),
    ("api-key", 8),
    ("api_key", 8),
    ("apikey", 8),
    ("x-auth-token", 8),
    ("auth-token", 8),
    ("auth_token", 8),
    ("access-token", 8),
    ("access_token", 8),
    ("refresh-token", 8),
    ("refresh_token", 8),
    ("id_token", 8),
    ("session-token", 8),
    ("session_token", 8),
    ("x-amz-security-token", 8),
    ("aws_secret_access_key", 8),
    ("client-secret", 8),
    ("client_secret", 8),
    ("secret_key", 8),
    ("private_key", 8),
    ("secret", 8),
    ("token", 8),
    ("password", 4),
    ("passwd", 4),
    ("pwd", 4),
    ("cookie", 8),
];

/// `key: value` and `key=value`, the key quoted or not, the value too.
fn pairs(text: &str, found: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    for (key, least) in SECRET_KEYS {
        for start in find_all(text, key) {
            let after_key = start + key.len();
            let boundary_before = start == 0 || !(wordish(bytes[start - 1]));
            let boundary_after = bytes
                .get(after_key)
                .is_none_or(|b| !wordish(*b) && *b != b'-');
            if !boundary_before || !boundary_after {
                continue;
            }
            let mut at = after_key;
            let skip = |at: &mut usize, wanted: &[u8]| {
                while bytes.get(*at).is_some_and(|b| wanted.contains(b)) {
                    *at += 1;
                }
            };
            skip(&mut at, b"\"' \t");
            if !matches!(bytes.get(at), Some(b':' | b'=')) {
                continue;
            }
            at += 1;
            skip(&mut at, b"> \t\"'");
            let cookie = *key == "cookie";
            let end = run(bytes, at, |b| {
                if cookie {
                    !matches!(b, b'\n' | b'\r' | b'"' | b'\'')
                } else {
                    !b.is_ascii_whitespace() && !b"\"',;&)}]<>".contains(&b)
                }
            });
            let value = &text[at..end];
            let long_enough = value.chars().count() >= *least;
            // A plain word after a generic key is prose, not a secret.
            let secret_like = *least < 8
                || cookie
                || value.chars().count() >= 16
                || value.bytes().any(|b| b.is_ascii_digit());
            if long_enough && secret_like {
                found.push((at, end));
            }
        }
    }
}

/// The password in `scheme://user:password@host`.
fn url_passwords(text: &str, found: &mut Vec<(usize, usize)>) {
    let mut from = 0;
    while let Some(at) = text[from..].find("://") {
        let start = from + at + 3;
        from = start;
        let authority_end = text[start..]
            .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
            .map_or(text.len(), |end| start + end);
        let authority = &text[start..authority_end];
        let Some(at_sign) = authority.rfind('@') else {
            continue;
        };
        if let Some(colon) = authority[..at_sign].find(':') {
            let (password_start, password_end) = (start + colon + 1, start + at_sign);
            if password_end > password_start {
                found.push((password_start, password_end));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(text: &str) -> String {
        hide(text).0
    }

    #[test]
    fn a_pasted_curl_command_keeps_its_shape_and_loses_its_secrets() {
        let curl = "curl --url 'https://api.example.com/v1/search' \\\n\
                    -H 'accept: application/json' \\\n\
                    -H 'authorization: Bearer\n\
                    eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlX2hlcmVfbG9uZw' \\\n\
                    -H 'x-api-key: 00000000-0000-4000-8000-000000000000' \\\n\
                    -H 'cookie: session=abcdef123456; theme=dark' \\\n\
                    --data-raw '{\"page\":1}'";
        let (out, count) = hide(curl);
        assert_eq!(
            out,
            "curl --url 'https://api.example.com/v1/search' \\\n\
             -H 'accept: application/json' \\\n\
             -H 'authorization: Bearer\n\
             [hidden by Muster]' \\\n\
             -H 'x-api-key: [hidden by Muster]' \\\n\
             -H 'cookie: [hidden by Muster]' \\\n\
             --data-raw '{\"page\":1}'"
        );
        assert_eq!(count, 3);
    }

    #[test]
    fn known_tokens_are_hidden_wherever_they_are() {
        for (text, kept) in [
            (
                "use ghp_abcdefghijklmnopqrstuvwxyz0123 here",
                "use [hidden by Muster] here",
            ),
            ("glpat-abcdefghijklmnop1234", "[hidden by Muster]"),
            (
                "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuv",
                "ANTHROPIC_API_KEY=[hidden by Muster]",
            ),
            ("key AKIAIOSFODNN7EXAMPLE end", "key [hidden by Muster] end"),
            (
                "AIzaSyA1234567890abcdefghijklmnopqrstu",
                "[hidden by Muster]",
            ),
            (
                "token lin_api_abcdefghijklmnopqrstuv",
                "token [hidden by Muster]",
            ),
            ("xoxb-1234567890-abcdefghij", "[hidden by Muster]"),
        ] {
            assert_eq!(hidden(text), kept, "{text}");
        }
    }

    #[test]
    fn secret_keys_hide_their_values_in_any_notation() {
        for (text, kept) in [
            ("password: hunter2", "password: [hidden by Muster]"),
            (
                "\"client_secret\": \"s3cr3t-value-here\"",
                "\"client_secret\": \"[hidden by Muster]\"",
            ),
            (
                "https://x.example/cb?access_token=abc123def456&state=ok",
                "https://x.example/cb?access_token=[hidden by Muster]&state=ok",
            ),
            ("API_KEY=0123456789abcdef", "API_KEY=[hidden by Muster]"),
            ("token => 'tok_1234567890'", "token => '[hidden by Muster]'"),
            (
                "Authorization: Basic dXNlcjpwYXNzd29yZA==",
                "Authorization: Basic [hidden by Muster]",
            ),
            (
                "https://ada:pa55word@git.example.com/repo.git",
                "https://ada:[hidden by Muster]@git.example.com/repo.git",
            ),
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----\nafter",
                "[hidden by Muster]\nafter",
            ),
        ] {
            assert_eq!(hidden(text), kept, "{text}");
        }
    }

    #[test]
    fn ordinary_ticket_text_is_left_as_it_is() {
        for text in [
            "The token expires after a day; the tokenizer splits words.",
            "Set password policy: strong. Secret: none.",
            "Test user: test_user_5@example.com, application APP2026/000001",
            "See https://example.atlassian.net/browse/APP-7 and #42.",
            "Bearer of bad news.",
            "Use a public key, -----BEGIN PUBLIC KEY----- is fine to share.",
            "The sk-1 build and AKIA are mentioned, too short to be keys.",
            "",
        ] {
            assert_eq!(hide(text), (text.to_owned(), 0), "{text}");
        }
    }

    #[test]
    fn any_text_at_all_is_read_without_failing() {
        for text in [
            "ü:ü=ü",
            "password:",
            "password: ",
            "token=",
            "Bearer",
            "Bearer ",
            "eyJ",
            "eyJ.eyJ.",
            "://",
            "://@",
            "a://b:@c",
            "-----BEGIN",
            "-----BEGIN PRIVATE KEY-----",
            "cookie:\n",
            "x-api-key:'ü",
            "ghp_",
            "日本語 token: 値123456789",
        ] {
            let _ = hide(text);
        }
        assert_eq!(hidden("password: ü1234"), "password: [hidden by Muster]");
    }
}
