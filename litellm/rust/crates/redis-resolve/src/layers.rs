use litellm_config::Value;
use litellm_env::Lookup;
use litellm_python_compat::{
    flags::{self, lower_is_true, str_to_bool},
    json::from_json,
};
use strum::IntoEnumIterator;

use crate::{
    Entry, Error, Kwargs,
    configured::{ConfiguredRedis, Nulls},
    is_present,
    keys::{self, Fallback, Kwarg, Read, When},
};

/// The configured settings over their `REDIS_*` values, `{**environment_kwargs, **env_overrides}`
/// in `_merge_redis_kwargs`, with the `get_secret` fallbacks it reads filled in. A null left in the result is
/// an explicit `None` that hid its environment value.
pub(crate) fn merged(configured: &ConfiguredRedis, lookup: &dyn Lookup) -> Result<Kwargs, Error> {
    let block: Kwargs = configured
        .fields
        .iter()
        .map(|(key, value)| (key, environment_reference(value, lookup)))
        .filter(|(key, value)| !value.is_null() || keeps_null(configured.nulls, key))
        .map(|(key, value)| {
            let value = serde_json::to_value(&value).map_err(|_| Error::NotPlainData {
                setting: key.clone(),
            })?;
            let entry = Entry {
                value,
                source: key.clone(),
            };
            Ok((key.clone(), entry))
        })
        .collect::<Result<_, Error>>()?;
    let url = Kwarg::Url.name();
    let names_a_target = Kwarg::iter()
        .filter(|kwarg| kwarg.setting().target && *kwarg != Kwarg::Url)
        .any(|kwarg| is_set(&block, kwarg.name()));
    let drop_environment_url = names_a_target && !is_set(&block, url);
    let layered: Kwargs = environment(lookup)
        .filter(|(key, _)| !(key == url && drop_environment_url))
        .chain(block)
        .collect();
    let fallbacks = fallbacks(&layered, configured.credential_provider.is_some(), lookup);
    Ok(layered.into_iter().chain(fallbacks).collect())
}

fn fallbacks(layered: &Kwargs, has_provider: bool, lookup: &dyn Lookup) -> Kwargs {
    let fill = |when: When| -> Kwargs {
        Kwarg::iter()
            .filter_map(|kwarg| {
                let fallback = kwarg.setting().fallback?;
                (fallback.when == when).then_some((kwarg, fallback))
            })
            .filter_map(|(kwarg, fallback)| filled(layered, kwarg, fallback, lookup))
            .collect()
    };
    let always = fill(When::Always);
    if has_provider || is_set(layered, Kwarg::CredentialProvider.name()) {
        return always;
    }
    let without_provider = fill(When::WithoutProvider);
    let value = |kwarg: Kwarg| {
        layered
            .get(kwarg.name())
            .filter(|entry| is_present(&entry.value))
            .or_else(|| without_provider.get(kwarg.name()))
            .map(|entry| &entry.value)
    };
    let gcp = value(Kwarg::GcpServiceAccount).is_some();
    let azure = value(Kwarg::AzureRedisAdToken)
        .is_some_and(|token| text(token).is_some_and(|token| lower_is_true(&token)));
    let aws = layered
        .get(Kwarg::AwsIamAuth.name())
        .is_some_and(|entry| flags::coerces_to_true(&from_json(entry.value.clone())));
    let azure_settings = match azure && !gcp {
        true => fill(When::AzureAd),
        false => Kwargs::new(),
    };
    let aws_settings = match aws && !gcp && !azure {
        true => fill(When::AwsIam),
        false => Kwargs::new(),
    };
    always
        .into_iter()
        .chain(without_provider)
        .chain(azure_settings)
        .chain(aws_settings)
        .collect()
}

fn filled(
    layered: &Kwargs,
    kwarg: Kwarg,
    fallback: Fallback,
    lookup: &dyn Lookup,
) -> Option<(String, Entry)> {
    if layered
        .get(kwarg.name())
        .is_some_and(|entry| is_present(&entry.value))
    {
        return None;
    }
    fallback.names.iter().find_map(|name| {
        let value = lookup
            .get(name)
            .filter(|value| !value.is_empty())
            .map(secret)?;
        match (fallback.read, &value) {
            (Read::SecretStr, serde_json::Value::Bool(_)) => None,
            _ => {
                let entry = Entry {
                    value,
                    source: (*name).to_owned(),
                };
                Some((kwarg.name().to_owned(), entry))
            }
        }
    })
}

fn secret(value: String) -> serde_json::Value {
    str_to_bool(&value).map_or(serde_json::Value::String(value), serde_json::Value::Bool)
}

fn text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn keeps_null(nulls: Nulls, key: &str) -> bool {
    match nulls {
        Nulls::Dropped => false,
        Nulls::KeptUnlessNamedByCache => {
            !keys::setting(key).is_some_and(|setting| setting.named_by_cache)
        }
        #[cfg(test)]
        Nulls::Kept => true,
    }
}

fn environment(lookup: &dyn Lookup) -> impl Iterator<Item = (String, Entry)> + '_ {
    Kwarg::iter().filter_map(|kwarg| {
        let source = kwarg.environment_name()?;
        let value = lookup.get(&source)?;
        let entry = Entry {
            value: secret(value),
            source,
        };
        Some((kwarg.name().to_owned(), entry))
    })
}

/// Whether the environment alone names where Redis is, under any name the merge reads a target from.
pub fn environment_names_a_target(lookup: &dyn Lookup) -> bool {
    Kwarg::iter()
        .filter(|kwarg| kwarg.setting().target)
        .flat_map(Kwarg::source_names)
        .any(|name| lookup.get(&name).is_some())
}

// load_config resolves os.environ/ to None when unset, so a missing reference is a null like any other
fn environment_reference(value: &Value, lookup: &dyn Lookup) -> Value {
    match value {
        Value::String(text) => match text.strip_prefix("os.environ/") {
            Some(name) => lookup.get(name).map_or(Value::Null, |value| {
                str_to_bool(&value).map_or(Value::String(value), Value::Bool)
            }),
            None => value.clone(),
        },
        _ => value.clone(),
    }
}

fn is_set(kwargs: &Kwargs, key: &str) -> bool {
    kwargs.get(key).is_some_and(|entry| !entry.value.is_null())
}
