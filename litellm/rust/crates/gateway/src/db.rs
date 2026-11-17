use std::collections::BTreeMap;

use litellm_config::Config;
use litellm_config_load::environment_values;
use litellm_db::Pools;
use litellm_db_resolve::ConfiguredDatabase;

use crate::Error;

pub(crate) async fn connect(config: &Config) -> Result<Option<Pools>, Error> {
    let variables = variables(config)?;
    let configured = match ConfiguredDatabase::read(&config.general_settings, &variables) {
        Ok(Some(configured)) => configured,
        Ok(None) => return Ok(None),
        Err(error @ litellm_db_resolve::Error::TokenAuth { .. }) => {
            tracing::warn!("{error}, so the gateway runs without a database pool");
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let database = configured.resolve().await?;
    for ignored in &database.ignored {
        tracing::warn!(
            "Database setting {} is ignored by the native pool: {}",
            ignored.setting,
            ignored.consequence
        );
    }
    Ok(Some(Pools::new(
        &database.writer,
        database.reader.as_ref(),
    )?))
}

fn variables(config: &Config) -> Result<BTreeMap<String, String>, Error> {
    let process = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    let configured = environment_values(&config.environment_variables)?
        .into_iter()
        .map(|(name, value)| (name, value.expose().to_owned()));
    Ok(process.chain(configured).collect())
}
