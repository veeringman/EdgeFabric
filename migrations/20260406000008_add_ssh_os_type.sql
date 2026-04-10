-- Add 'ssh' to the device_os enum for discovered SSH servers on the network.
ALTER TYPE device_os ADD VALUE IF NOT EXISTS 'ssh';
