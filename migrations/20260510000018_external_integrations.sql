-- External integration endpoints for MQTT and cloud sink bridging.

CREATE TABLE IF NOT EXISTS integration_endpoints (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    transport        TEXT NOT NULL,
    kind             TEXT NOT NULL,
    enabled          BOOLEAN NOT NULL DEFAULT TRUE,
    config           JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_error       TEXT,
    last_success_at  TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT integration_endpoints_transport_ck CHECK (
        transport IN ('mqtt_subscription', 'mqtt_publisher', 'cloud_sink')
    ),
    CONSTRAINT integration_endpoints_kind_ck CHECK (
        kind IN (
            'mqtt_subscription',
            'mqtt_publisher',
            'aws_s3_presigned_put',
            'aws_iot_core_https',
            'azure_iot_hub'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_integration_endpoints_tenant_transport
    ON integration_endpoints (tenant_id, transport);

CREATE INDEX IF NOT EXISTS idx_integration_endpoints_enabled
    ON integration_endpoints (enabled)
    WHERE enabled = TRUE;

CREATE TABLE IF NOT EXISTS integration_delivery_attempts (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint_id   UUID NOT NULL REFERENCES integration_endpoints(id) ON DELETE CASCADE,
    tenant_id     UUID REFERENCES tenants(id) ON DELETE SET NULL,
    device_id     UUID,
    sensor        TEXT,
    payload       JSONB NOT NULL,
    status        TEXT NOT NULL,
    attempt       INTEGER NOT NULL,
    error_message TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT integration_delivery_attempts_status_ck CHECK (
        status IN ('success', 'failed', 'dead_letter')
    )
);

CREATE INDEX IF NOT EXISTS idx_integration_delivery_endpoint_created
    ON integration_delivery_attempts (endpoint_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_integration_delivery_status_created
    ON integration_delivery_attempts (status, created_at DESC);
