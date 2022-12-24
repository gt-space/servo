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

CREATE TABLE IF NOT EXISTS Tests (
	test_id TEXT NOT NULL PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS RequestLogs (
	log_id TEXT NOT NULL PRIMARY KEY,
	endpoint TEXT NOT NULL,
	origin TEXT NOT NULL,
	username TEXT REFERENCES Users(username),
	status_code INTEGER,
	timestamp INTEGER NOT NULL CHECK(timestamp > 0)
);

CREATE TABLE IF NOT EXISTS TestLogs (
	log_id TEXT NOT NULL PRIMARY KEY,
	test_id TEXT NOT NULL REFERENCES Tests(test_id),
	initiator TEXT NOT NULL REFERENCES Users(username),
	start_time INTEGER NOT NULL CHECK(start_time > 0),
	end_time INTEGER CHECK(end_time >= start_time),
	did_pass INTEGER CHECK(did_pass BETWEEN 0 AND 1),
	message TEXT
);

-- TRIGGERS --
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
