-- PRAGMAS --
PRAGMA foreign_keys = ON;

-- TABLES --
CREATE TABLE IF NOT EXISTS Users (
	username TEXT NOT NULL PRIMARY KEY,
	pass_hash TEXT,
	pass_salt TEXT NOT NULL,
	is_admin INTEGER NOT NULL CHECK(is_admin BETWEEN 0 AND 1)
);

CREATE TABLE IF NOT EXISTS Sessions (
	session_id TEXT NOT NULL PRIMARY KEY,
	username TEXT NOT NULL UNIQUE REFERENCES Users(username),
	timestamp INTEGER NOT NULL CHECK(timestamp > 0)
);

CREATE TABLE IF NOT EXISTS ForwardingTargets (
	target_id TEXT NOT NULL PRIMARY KEY,
	socket_address TEXT NOT NULL UNIQUE,
	expiration INTEGER NOT NULL CHECK(expiration > 0)
);

CREATE TABLE IF NOT EXISTS RequestLogs (
	log_id TEXT NOT NULL PRIMARY KEY,
	endpoint TEXT NOT NULL,
	origin TEXT NOT NULL,
	username TEXT REFERENCES Users(username),
	status_code INTEGER,
	timestamp INTEGER NOT NULL CHECK(timestamp > 0)
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

CREATE TRIGGER IF NOT EXISTS overwrite_previous_session
BEFORE INSERT ON Sessions
BEGIN
	DELETE FROM Sessions WHERE username = new.username;
END;

-- COMMANDS --
INSERT OR IGNORE INTO Users VALUES ('root', NULL, 'F3sBIV7QQVWq948F4heYhg', 1);
