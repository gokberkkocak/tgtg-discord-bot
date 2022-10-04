-- Add migration script here
CREATE TABLE IF NOT EXISTS channels (
    id                           INTEGER PRIMARY KEY NOT NULL,
    channel_id                   TEXT NOT NULL,
    latitude                     REAL NOT NULL,
    longitude                    REAL NOT NULL,
    radius                       INTEGER NOT NULL,
    regex                        TEXT,
    active                       INTEGER NOT NULL
);