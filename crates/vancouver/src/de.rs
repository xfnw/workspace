use std::collections::BTreeSet;

use serde::{
    Deserialize, Deserializer,
    de::{Visitor, value},
};

pub fn string_or_bset<'de, D>(deserializer: D) -> Result<BTreeSet<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrBSet;

    impl<'de> Visitor<'de> for StringOrBSet {
        type Value = BTreeSet<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or set of string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(BTreeSet::from([v.to_string()]))
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            Deserialize::deserialize(value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrBSet)
}
