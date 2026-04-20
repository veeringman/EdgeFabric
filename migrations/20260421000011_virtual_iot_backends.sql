CREATE TABLE IF NOT EXISTS virtual_iot_backends (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name             TEXT NOT NULL UNIQUE,
    provider         TEXT NOT NULL,
    controller_type  TEXT NOT NULL,
    region           TEXT NOT NULL DEFAULT 'local',
    status           TEXT NOT NULL DEFAULT 'available',
    vm_name          TEXT,
    shell_host       TEXT,
    shell_port       INTEGER,
    config           JSONB NOT NULL DEFAULT '{}'::jsonb,
    capabilities     TEXT[] NOT NULL DEFAULT '{}',
    attached_device_id UUID REFERENCES devices(id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_virtual_iot_backends_controller_type
    ON virtual_iot_backends (controller_type);

CREATE INDEX IF NOT EXISTS idx_virtual_iot_backends_status
    ON virtual_iot_backends (status);

CREATE INDEX IF NOT EXISTS idx_virtual_iot_backends_attached_device_id
    ON virtual_iot_backends (attached_device_id);

ALTER TABLE virtual_iot_configs
ADD COLUMN IF NOT EXISTS backend_id UUID REFERENCES virtual_iot_backends(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_virtual_iot_configs_backend_id
    ON virtual_iot_configs (backend_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_trigger
        WHERE tgname = 'trg_virtual_iot_backends_updated'
    ) THEN
        CREATE TRIGGER trg_virtual_iot_backends_updated
        BEFORE UPDATE ON virtual_iot_backends
        FOR EACH ROW EXECUTE FUNCTION set_updated_at();
    END IF;
END $$;