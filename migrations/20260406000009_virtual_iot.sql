-- Add 'virtual_iot' to the device_os enum and create virtual_iot_configs table.

ALTER TYPE device_os ADD VALUE IF NOT EXISTS 'virtual_iot';

CREATE TABLE IF NOT EXISTS virtual_iot_configs (
    device_id       UUID PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    controller_type TEXT NOT NULL DEFAULT 'esp32-devkit',
    enabled_sensors JSONB NOT NULL DEFAULT '[]'::jsonb,
    backend         JSONB NOT NULL DEFAULT '{}'::jsonb,
    sim_rate_ms     INTEGER NOT NULL DEFAULT 500,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
