//! What the body check asks of the process it runs in: how the consumer of a body decodes it, and
//! what the proxy has configured.

use super::value::Params;

pub trait Host {
    type Error;
    type Deployments: Deployments<Error = Self::Error>;

    /// A JSON-encoded mapping, decoded the way the code that later reads the body decodes it.
    /// `None` when the text is not a mapping.
    fn decode(&self, text: &str) -> Result<Option<Params>, Self::Error>;

    /// The mapping a form's `prefix`-spelled keys (`metadata[tags]`) build.
    fn form_metadata(&self, prefix: &str) -> Result<Params, Self::Error>;

    /// The values a stored credential holds, empty when none has this name.
    fn credential(&self, name: &str) -> Result<Params, Self::Error>;

    /// Whether a fallback target that reads as a URL points at a host the proxy may call.
    fn url_destination_allowed(&self, value: &str) -> Result<bool, Self::Error>;

    /// The configured deployments, or `None` on a proxy that routes none.
    fn deployments(&self) -> Option<&Self::Deployments>;
}

/// The router's deployments as the caller's team sees them.
pub trait Deployments {
    type Error;

    /// The deployments that can serve `model`.
    fn group(&self, model: &str) -> Result<Vec<Deployment>, Self::Error>;

    /// The model group `name` is an alias of.
    fn alias(&self, name: &str) -> Result<Option<String>, Self::Error>;

    /// Every model group a failure on `name` can fall back to, through chained and generic
    /// fallbacks of every kind.
    fn fallbacks(&self, name: &str) -> Result<Vec<String>, Self::Error>;

    /// Whether `value` is the `api_base` a deployment's opt-in pattern admits.
    fn api_base_matches(&self, pattern: &str, value: &str) -> Result<bool, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deployment {
    pub model_name: String,
    /// `None` when the deployment's provider cannot be told.
    pub provider: Option<String>,
    /// `configurable_clientside_auth_params`, or `None` when the deployment sets none.
    pub opt_in: Option<OptIn>,
}

/// The overrides a deployment lets a caller set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptIn {
    pub params: Vec<String>,
    pub api_base_patterns: Vec<String>,
}
