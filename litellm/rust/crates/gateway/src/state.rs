use std::sync::Arc;

use axum::extract::FromRef;
use litellm_config::Config;
use litellm_db::Pools;
use litellm_gateway_inference::Gateway;

use crate::{Error, RedisClients, db};

#[derive(Clone)]
pub struct AppState {
    pub inference: Arc<Gateway>,
    pub redis: RedisClients,
    pub database: Option<Pools>,
}

impl AppState {
    pub async fn build(config: &Config) -> Result<Self, Error> {
        let inference = crate::build_inference(config)?;
        let redis = crate::build_redis(config, &inference.resources.pool, &inference.http).await?;
        let database = db::connect(config).await?;
        Ok(Self {
            inference,
            redis,
            database,
        })
    }

    pub async fn close(&self) {
        if let Some(database) = &self.database {
            database.close().await;
        }
    }
}

impl FromRef<AppState> for RedisClients {
    fn from_ref(state: &AppState) -> Self {
        state.redis.clone()
    }
}

impl FromRef<AppState> for Option<Pools> {
    fn from_ref(state: &AppState) -> Self {
        state.database.clone()
    }
}
