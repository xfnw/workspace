// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use std::{collections::BTreeMap, str::FromStr};

pub struct InfluxLine {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub fields: BTreeMap<String, f64>,
    pub _timestamp: Option<f64>,
}

impl InfluxLine {
    pub fn parse(inp: &str) -> Option<Self> {
        let (name, rest) = parse_escaped_until(inp, |c| c == ',' || c == ' ');

        let mut labels = BTreeMap::new();
        let rest = if let Some((',', rest)) = chomp(rest) {
            let (mut new_labels, rest) = parse_kv_list(rest, |c| c == ' ')?;
            labels.append(&mut new_labels);
            rest
        } else {
            rest
        };

        let Some((' ', rest)) = chomp(rest) else {
            return None;
        };

        let mut fields = BTreeMap::new();
        let (mut new_fields, rest) = parse_kv_list(rest, |c| c == ' ')?;
        fields.append(&mut new_fields);

        let timestamp = if let Some((c, rest)) = chomp(rest) {
            if c != ' ' {
                return None;
            }
            Some(rest.parse().ok()?)
        } else {
            None
        };

        Some(Self {
            name,
            labels,
            fields,
            _timestamp: timestamp,
        })
    }
}

fn parse_escaped_until(inp: &str, end_predicate: impl Fn(char) -> bool) -> (String, &str) {
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
            return (out, prev_rest);
        }
        if c == '\\' {
            backslashed = true;
        } else {
            out.push(c);
        }
    }

    if backslashed {
        out.push('\\');
    }

    (out, chars.as_str())
}

fn chomp(inp: &str) -> Option<(char, &str)> {
    let mut chars = inp.chars();
    Some((chars.next()?, chars.as_str()))
}

fn parse_quoted(inp: &str) -> Option<(String, &str)> {
    let Some(('"', rest)) = chomp(inp) else {
        return None;
    };
    let (s, rest) = parse_escaped_until(rest, |c| c == '"');
    let Some(('"', rest)) = chomp(rest) else {
        return None;
    };
    Some((s, rest))
}

fn parse_maybe_quoted(inp: &str, end_predicate: impl Fn(char) -> bool) -> Option<(String, &str)> {
    if inp.starts_with('"') {
        return parse_quoted(inp);
    }
    Some(parse_escaped_until(inp, end_predicate))
}

fn parse_kv<V: FromStr>(
    inp: &str,
    end_predicate: impl Fn(char) -> bool,
) -> Option<(String, V, &str)> {
    let (key, rest) = parse_maybe_quoted(inp, |c| end_predicate(c) || c == '=')?;
    let Some(('=', rest)) = chomp(rest) else {
        return None;
    };
    let (value, rest) = parse_maybe_quoted(rest, end_predicate)?;
    Some((key, value.parse().ok()?, rest))
}

fn parse_kv_list<V: FromStr>(
    mut inp: &str,
    end_predicate: impl Fn(char) -> bool,
) -> Option<(BTreeMap<String, V>, &str)> {
    let mut out = BTreeMap::new();
    loop {
        let (key, value, rest) = parse_kv(inp, |c| end_predicate(c) || c == ',')?;
        out.insert(key, value);
        inp = match chomp(rest) {
            Some((',', rest)) => rest,
            Some((c, _)) => return end_predicate(c).then_some((out, rest)),
            None => return Some((out, rest)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_list() {
        let (map, rest) = parse_kv_list::<f64>(
            r#"bark=1,i\ have\ spaces=2,"im quoted"=3,"have a \backslash escaped quote: \""=4 meow"#,
            |c| c == ' ',
        )
        .unwrap();
        assert_eq!(rest, " meow");
        let mut iter = map.iter();
        assert_eq!(iter.next(), Some((&"bark".to_string(), &1f64)));
        assert_eq!(
            iter.next(),
            Some((&"have a \\backslash escaped quote: \"".to_string(), &4f64))
        );
        assert_eq!(iter.next(), Some((&"i have spaces".to_string(), &2f64)));
        assert_eq!(iter.next(), Some((&"im quoted".to_string(), &3f64)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn end() {
        assert_eq!(
            parse_maybe_quoted("meow", |_| false),
            Some(("meow".to_string(), ""))
        );
        assert_eq!(parse_maybe_quoted("\"meow", |_| false), None);
        assert_eq!(
            parse_maybe_quoted("\"meow\"", |_| false),
            Some(("meow".to_string(), ""))
        );
    }

    #[test]
    fn parse_line() {
        let line = InfluxLine::parse(r#"meows,name="vulpine",species=fox total=6"#).unwrap();
        assert_eq!(line.name, "meows");

        let mut expected_labels = BTreeMap::new();
        expected_labels.insert("name".to_string(), "vulpine".to_string());
        expected_labels.insert("species".to_string(), "fox".to_string());
        assert_eq!(line.labels, expected_labels);

        let mut expected_fields = BTreeMap::new();
        expected_fields.insert("total".to_string(), 6f64);
        assert_eq!(line.fields, expected_fields);
    }
}
