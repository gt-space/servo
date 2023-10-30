-- PRAGMAS --
PRAGMA foreign_keys = ON;

-- TABLES --
CREATE TABLE IF NOT EXISTS ForwardingTargets (
	target_id TEXT NOT NULL PRIMARY KEY,
	socket_address TEXT NOT NULL UNIQUE,
	expiration INTEGER NOT NULL CHECK(expiration > 0)
);

CREATE TABLE IF NOT EXISTS RequestLogs (
	log_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
	endpoint TEXT NOT NULL,
	origin TEXT NOT NULL,
	hostname TEXT,
	status_code INTEGER DEFAULT NULL,
	timestamp INTEGER NOT NULL DEFAULT (unixepoch()) CHECK(timestamp > 0) 
);

CREATE TABLE IF NOT EXISTS DataLogs (
	log_id INTEGER NOT NULL PRIMARY KEY,
	raw_accumulated BLOB NOT NULL,
	frame_split_indices BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS NodeMappings (
	text_id TEXT NOT NULL PRIMARY KEY,
	node_id INTEGER NOT NULL,
	board_id INTEGER NOT NULL,
	channel INTEGER NOT NULL
);

-- TRIGGERS --
CREATE TRIGGER IF NOT EXISTS update_forwarding
AFTER UPDATE ON ForwardingTargets
WHEN old.socket_address != new.socket_address
BEGIN
	SELECT forward_target(old.socket_address, 0);
	SELECT forward_target(new.socket_address, 1);
END;

CREATE TRIGGER IF NOT EXISTS add_forwarding
AFTER INSERT ON ForwardingTargets
BEGIN
	SELECT forward_target(new.socket_address, 1);
END;

CREATE TRIGGER IF NOT EXISTS remove_forwarding
AFTER DELETE ON ForwardingTargets
BEGIN
	SELECT forward_target(old.socket_address, 0);
END;

CREATE TRIGGER IF NOT EXISTS no_update_request_logs
BEFORE UPDATE ON RequestLogs
WHEN old.status_code IS NOT NULL
BEGIN
	SELECT RAISE(ABORT, 'Updating request logs is not permitted.');
END;

CREATE TRIGGER IF NOT EXISTS no_delete_request_logs
BEFORE DELETE ON RequestLogs
BEGIN
	SELECT RAISE(ABORT, 'Deleting request logs is not permitted.');
END;

-- COMMANDS --
DELETE FROM ForwardingTargets;
