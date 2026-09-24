-- Threadrinth: Modrinth's anonymous usage statistics are off by default.
-- Runs once per install, so turning them back on in Settings → Privacy sticks.
UPDATE settings SET telemetry = FALSE;
