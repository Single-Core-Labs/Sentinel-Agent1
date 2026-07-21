"""Placeholder for OpenTelemetry provider configuration.

The original code imported ``provider`` to expose exporter factories.  The
stub remains empty because the rest of the package never calls into it after
the observability stubs above.  Keeping the file prevents ``ImportError`` if any
future code attempts to import it.
"""
