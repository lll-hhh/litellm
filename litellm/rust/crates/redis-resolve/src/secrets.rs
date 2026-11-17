use std::collections::{BTreeMap, BTreeSet};

use litellm_config::Value;
use litellm_env::Lookup;
use litellm_secret_value::SecretValue;
use strum::IntoEnumIterator;

use crate::{configured::ConfiguredRedis, keys::Kwarg};

#[derive(Clone, Debug, Default)]
pub struct SecretSnapshot(BTreeMap<String, Option<SecretValue>>);

impl SecretSnapshot {
    pub fn read(names: BTreeSet<String>, lookup: &dyn Lookup) -> Self {
        Self(
            names
                .into_iter()
                .map(|name| {
                    let value = lookup.get(&name).map(SecretValue::new);
                    (name, value)
                })
                .collect(),
        )
    }
}

impl Lookup for SecretSnapshot {
    fn get(&self, name: &str) -> Option<String> {
        match self.0.get(name) {
            Some(value) => value.as_ref().map(|value| value.expose().to_owned()),
            None => {
                debug_assert!(
                    false,
                    "{name} is missing from ConfiguredRedis::secret_names"
                );
                tracing::error!(
                    name,
                    "a Redis secret was read without being in the snapshot"
                );
                None
            }
        }
    }
}

impl ConfiguredRedis {
    pub fn secret_names(&self) -> BTreeSet<String> {
        let references = self.fields.values().filter_map(|value| match value {
            Value::String(text) => text.strip_prefix("os.environ/").map(str::to_owned),
            _ => None,
        });
        let declared = Kwarg::iter().flat_map(Kwarg::source_names);
        declared.chain(references).collect()
    }
}
