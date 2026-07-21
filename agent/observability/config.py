"""Observability configuration model.

The original repository used a full OpenTelemetry configuration, but the
module has been removed.  This stub provides a minimal Pydantic model that
matches the fields accessed by the rest of the code base.  All fields are
optional and extra keys are allowed so that future extensions do not break
this fallback implementation.
"""

from pydantic import BaseModel


class ObservabilityConfig(BaseModel):
    """Simple config placeholder.

    * ``enabled`` – whether OpenTelemetry should be active (default ``False``).
    * ``provider`` – optional string identifying the OTel exporter.
    * Any additional keys are accepted via ``extra = "allow"``.
    """

    enabled: bool = False
    provider: str | None = None

    class Config:
        extra = "allow"
