-- Add ip_address column to devices so agents can report device network addresses
-- and the streaming handler can auto-connect via ADB TCP.
ALTER TABLE devices ADD COLUMN IF NOT EXISTS ip_address TEXT;
