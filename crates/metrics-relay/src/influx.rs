// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

pub struct InfluxLine {
    name: String,
}

impl InfluxLine {
    pub fn parse(inp: &str) -> Option<Self> {
        todo!()
    }
}

fn parse_escaped_until(inp: &str, end_predicate: impl Fn(char) -> bool) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = inp.chars();
    let mut backslashed = false;

    while let prev_rest = chars.as_str()
        && let Some(c) = chars.next()
    {
        if backslashed {
            backslashed = false;
            match c {
                'r' => out.push('\r'),
                'n' => out.push('\n'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                _ => {
                    // influxdb is silly
                    // https://docs.influxdata.com/influxdb/v2/reference/syntax/line-protocol/#escaping-backslashes
                    if !end_predicate(c) {
                        out.push('\\');
                    }
                    out.push(c);
                }
            }
            continue;
        }
        if end_predicate(c) {
            return Some((out, prev_rest));
        }
        if c == '\\' {
            backslashed = true;
        } else {
            out.push(c);
        }
    }

    None
}

fn parse_quoted(inp: &str) -> Option<(String, &str)> {
    let mut chars = inp.chars();
    if chars.next().is_none_or(|c| c != '"') {
        return None;
    }
    let (s, rest) = parse_escaped_until(chars.as_str(), |c| c == '"')?;
    assert_eq!(&rest[..1], "\"");
    Some((s, &rest[1..]))
}
