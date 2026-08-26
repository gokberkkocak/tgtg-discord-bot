-- Bot state: per-channel runtime flag plus unified watches.
-- Location watches cover an area; store watches pin a single shop item.
CREATE TABLE channels (
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
