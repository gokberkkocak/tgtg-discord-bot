-- Unified /watch system:
-- channels keeps only runtime state (active flag), location config moves to watches.
-- Old per-channel rows become 'location' watches; duplicate channel rows are merged.
CREATE TABLE channels_new (
    channel_id TEXT PRIMARY KEY NOT NULL,
    active     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE watches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    channel_id  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('location','store')),
    latitude    REAL,
    longitude   REAL,
    radius      INTEGER,
    regex       TEXT,
    store_id    TEXT,
    item_id     TEXT,
    store_name  TEXT
);

INSERT INTO channels_new (channel_id, active)
    SELECT channel_id, MAX(active) FROM channels GROUP BY channel_id;

INSERT INTO watches (channel_id, kind, latitude, longitude, radius, regex)
    SELECT channel_id, 'location', latitude, longitude, radius, regex FROM channels;

DROP TABLE channels;
ALTER TABLE channels_new RENAME TO channels;
