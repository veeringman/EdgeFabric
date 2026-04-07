-- Make serial a unique constraint so agent upserts work (ON CONFLICT).
DROP INDEX IF EXISTS idx_devices_serial;
CREATE UNIQUE INDEX idx_devices_serial ON devices (serial) WHERE serial IS NOT NULL;
