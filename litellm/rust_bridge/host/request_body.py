"""What the request body check asks the proxy: how the code that later reads a body decodes it, and
what the router and the credential store hold."""

from collections.abc import Mapping
from typing import Final, TypeAlias

from pydantic import TypeAdapter, ValidationError

import litellm
from litellm import Router
from litellm.litellm_core_utils.credential_accessor import CredentialAccessor
from litellm.litellm_core_utils.safe_json_loads import safe_json_loads
from litellm.litellm_core_utils.url_utils import is_url_destination_allowed_by_host, provider_url_destination_candidates
from litellm.proxy.auth.auth_utils import (
    EVERY_FALLBACK_TYPE,
    check_regex_or_str_match,
    deployment_clientside_auth_params,
    deployment_provider,
    model_group_deployments,
    router_fallback_model_names,
)
from litellm.proxy.common_utils.http_parsing_utils import extract_nested_form_metadata
from litellm.types.router import LiteLLMParamsTypedDict

_MAPPING: Final = TypeAdapter(Mapping[str, object])
_ALLOWED_HOSTS: Final = TypeAdapter(tuple[str, ...])

OptIn: TypeAlias = tuple[tuple[str, ...], tuple[str, ...]]
GroupDeployment: TypeAlias = tuple[str, str | None, OptIn | None]


def decode_mapping(text: str) -> Mapping[str, object] | None:
    try:
        return _MAPPING.validate_python(safe_json_loads(text))
    except ValidationError:
        return None


def form_metadata(body: Mapping[str, object], prefix: str) -> Mapping[str, object]:
    return extract_nested_form_metadata(form_data=body, prefix=prefix)


def credential_values(name: str) -> Mapping[str, object]:
    return _MAPPING.validate_python(CredentialAccessor.get_credential_values(name))


def url_destination_allowed(value: str) -> bool:
    allowed_hosts: Final = _ALLOWED_HOSTS.validate_python(
        getattr(litellm, "provider_url_destination_allowed_hosts", None) or ()
    )
    return all(
        is_url_destination_allowed_by_host(candidate, list(allowed_hosts))
        for candidate in provider_url_destination_candidates(value)
        if candidate.lower().startswith(("http://", "https://"))
    )


def _opt_in(litellm_params: LiteLLMParamsTypedDict) -> OptIn | None:
    items: Final = deployment_clientside_auth_params(litellm_params)
    if items is None:
        return None
    return (
        tuple(item for item in items if isinstance(item, str)),
        tuple(item["api_base"] for item in items if not isinstance(item, str)),
    )


def group_deployments(router: Router, model: str, team_id: str | None) -> tuple[GroupDeployment, ...]:
    return tuple(
        (
            deployment["model_name"],
            deployment_provider(deployment["litellm_params"]),
            _opt_in(deployment["litellm_params"]),
        )
        for deployment in model_group_deployments(model, router, team_id)
    )


def alias(router: Router, name: str) -> str | None:
    return router.registry().resolve(name, kind="alias") or None


def fallbacks(router: Router, name: str) -> tuple[str, ...]:
    return tuple(sorted(router_fallback_model_names(name, router, EVERY_FALLBACK_TYPE)))


def api_base_matches(pattern: str, value: str) -> bool:
    return check_regex_or_str_match(request_body_value=value, regex_str=pattern)
