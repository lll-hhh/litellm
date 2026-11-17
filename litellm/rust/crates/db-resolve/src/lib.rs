mod endpoint;
mod error;
mod location;
mod params;
mod probe;
mod settings;

use std::collections::BTreeMap;

use litellm_config::GeneralSettings;
use litellm_db::Endpoint;

pub use endpoint::{Limits, Overlay, Rule};
pub use error::{Error, Expected};
pub use location::{REPLICA_URL, WRITER_URL};

use location::Location;
use settings::PoolSettings;

const TOKEN_AUTH_FLAGS: [&str; 2] = ["IAM_TOKEN_DB_AUTH", "AZURE_POSTGRESQL_AUTH"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ignored {
    pub setting: String,
    pub consequence: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Database {
    pub writer: Endpoint,
    pub reader: Option<Endpoint>,
    pub ignored: Vec<Ignored>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfiguredDatabase {
    writer: Location,
    reader: Option<Location>,
    pool: PoolSettings,
}

impl ConfiguredDatabase {
    pub fn read(
        general: &GeneralSettings,
        variables: &BTreeMap<String, String>,
    ) -> Result<Option<Self>, Error> {
        for variable in TOKEN_AUTH_FLAGS {
            if settings::flag(variable, variables.get(variable).map(String::as_str))? {
                return Err(Error::TokenAuth { variable });
            }
        }
        let Some(writer) = location::writer(general.database_url.as_ref(), variables)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            writer,
            reader: location::reader(variables)?,
            pool: PoolSettings::read(general, variables)?,
        }))
    }

    pub async fn resolve(&self) -> Result<Database, Error> {
        let (writer, writer_ignored) = resolved(&self.writer, &self.pool.writer()).await?;
        let (reader, reader_ignored) = match &self.reader {
            Some(location) => {
                let (endpoint, ignored) = resolved(location, &self.pool.replica()).await?;
                (Some(endpoint), ignored)
            }
            None => (None, Vec::new()),
        };
        Ok(Database {
            writer,
            reader,
            ignored: writer_ignored.into_iter().chain(reader_ignored).collect(),
        })
    }
}

/// The pool's endpoint for the final url a host connects with, named `source` in errors, under what the host
/// sets on top. Both hosts share it, and it asks the server about TLS where it has to.
pub async fn interpret(
    source: &'static str,
    url: &str,
    overlay: &Overlay,
) -> Result<(Endpoint, Vec<Ignored>), Error> {
    resolved(&Location::from_url(source, url)?, overlay).await
}

async fn resolved(
    location: &Location,
    overlay: &Overlay,
) -> Result<(Endpoint, Vec<Ignored>), Error> {
    let tls_offered = !endpoint::needs_tls_probe(location)
        || probe::server_offers_tls(&location.host, location.port).await;
    endpoint::endpoint(location, overlay, tls_offered)
}
