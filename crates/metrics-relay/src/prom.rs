// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use std::fmt::{Display, Formatter, Result, Write};

struct EscapeString<'a>(&'a str);

impl Display for EscapeString<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('"')?;

        for c in self.0.chars() {
            match c {
                '\\' => f.write_str("\\\\"),
                '"' => f.write_str("\\\""),
                '\r' => f.write_str("\\r"),
                '\n' => f.write_str("\\n"),
                _ => f.write_char(c),
            }?;
        }

        f.write_char('"')
    }
}

struct EscapeMetric<'a>(&'a str, Option<&'a str>);

impl Display for EscapeMetric<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut chars = self.0.chars();
        f.write_char(
            chars
                .next()
                .filter(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '_' | ':'))
                .unwrap_or('_'),
        )?;
        for c in chars {
            f.write_char(match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':' => c,
                _ => '_',
            })?;
        }

        if let Some(extra) = self.1 {
            f.write_char('_')?;
            for c in extra.chars() {
                f.write_char(match c {
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':' => c,
                    _ => '_',
                })?;
            }
        }

        Ok(())
    }
}

pub struct NameAndLabels<'a, I> {
    pub name: &'a str,
    pub extra_name: Option<&'a str>,
    pub labels: &'a I,
}

impl<'a, I, N, V> Display for NameAndLabels<'a, I>
where
    &'a I: IntoIterator<Item = (N, V)>,
    N: AsRef<str>,
    V: AsRef<str>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", EscapeMetric(self.name, self.extra_name))?;

        let mut iter = self.labels.into_iter();
        if let Some(first) = iter.next() {
            f.write_char('{')?;
            write!(
                f,
                "{}={}",
                EscapeString(first.0.as_ref()),
                EscapeString(first.1.as_ref())
            )?;

            for (label, value) in iter {
                write!(
                    f,
                    ",{}={}",
                    EscapeString(label.as_ref()),
                    EscapeString(value.as_ref())
                )?;
            }

            f.write_char('}')?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn format_name_and_labels() {
        let mut labels = BTreeMap::new();
        labels.insert("cat", "klea");
        labels.insert("backslash: \\", "newline: \n");
        let nal = NameAndLabels {
            name: "meows",
            extra_name: Some("total"),
            labels: &labels,
        };
        assert_eq!(
            nal.to_string(),
            r#"meows_total{"backslash: \\"="newline: \n","cat"="klea"}"#
        );
    }
}
