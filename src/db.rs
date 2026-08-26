use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::{Context as _, Result, bail};
use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::model::id::ChannelId;
use sqlx::SqlitePool;

use crate::data::{Watch, WatchDetails};

pub struct BotDB {
    pool: SqlitePool,
}

struct StoreRow {
    store_id: String,
    item_id: String,
    store_name: String,
}

impl WatchDetails {
    fn as_store_row(&self) -> Option<StoreRow> {
        match self {
            WatchDetails::Store {
                store_id,
                item_id,
                store_name,
                ..
            } => Some(StoreRow {
                store_id: store_id.clone(),
                item_id: item_id.clone(),
                store_name: store_name.clone(),
            }),
            _ => None,
        }
    }
}

impl BotDB {
    pub async fn new(db_url: &str) -> Result<Self> {
        let token_db = BotDB {
            pool: SqlitePool::connect(db_url).await?,
        };
        Ok(token_db)
    }

    /// Loads all watches grouped per channel plus the set of active channels.
    /// Rows violating the watch model fail startup.
    pub async fn load_all(&self) -> Result<(HashMap<ChannelId, Vec<Watch>>, HashSet<ChannelId>)> {
        let mut conn = self.pool.acquire().await?;
        let watch_records = sqlx::query!(
            r#"
                SELECT id, channel_id, kind, latitude, longitude, radius, regex, store_id, item_id, store_name
                FROM watches ORDER BY id
            "#
        )
        .fetch_all(&mut *conn)
        .await?;
        let channel_records = sqlx::query!(
            r#"
                SELECT channel_id, active FROM channels
            "#
        )
        .fetch_all(&mut *conn)
        .await?;

        let mut watches: HashMap<ChannelId, Vec<Watch>> = HashMap::new();
        for r in watch_records {
            let Ok(channel_id) = ChannelId::from_str(&r.channel_id) else {
                bail!("Invalid channel id: {}", r.channel_id);
            };
            let details = match r.kind.as_str() {
                "location" => WatchDetails::Location {
                    latitude: r.latitude.context("location watch without latitude")?,
                    longitude: r.longitude.context("location watch without longitude")?,
                    radius: u8::try_from(r.radius.context("location watch without radius")?)
                        .context("invalid radius")?,
                    regex: r.regex.as_deref().map(Regex::new).transpose()?,
                },
                "store" => WatchDetails::Store {
                    store_id: r.store_id.context("store watch without store id")?,
                    item_id: r.item_id.context("store watch without item id")?,
                    store_name: r.store_name.context("store watch without store name")?,
                    latitude: r.latitude.unwrap_or_default(),
                    longitude: r.longitude.unwrap_or_default(),
                },
                other => bail!("Unknown watch kind: {}", other),
            };
            watches.entry(channel_id).or_default().push(Watch {
                id: r.id,
                channel_id,
                details,
            });
        }
        let active_set = channel_records
            .iter()
            .filter(|r| r.active == 1)
            .filter_map(|r| ChannelId::from_str(&r.channel_id).ok())
            .collect();
        Ok((watches, active_set))
    }

    /// Inserts a watch and returns it enriched with its database assigned id.
    /// Also makes sure a (passive) channel row exists.
    pub async fn add_watch(&self, watch: &Watch) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = watch.channel_id.to_string();
        sqlx::query!(
            r#"INSERT OR IGNORE INTO channels (channel_id, active) VALUES (?1, 0)"#,
            channel_id_str,
        )
        .execute(&mut *conn)
        .await?;
        let (kind, latitude, longitude, radius, regex) = match &watch.details {
            WatchDetails::Location {
                latitude,
                longitude,
                radius,
                regex,
            } => (
                "location",
                Some(*latitude),
                Some(*longitude),
                Some(*radius as i64),
                regex.as_ref().map(|r| r.as_str().to_owned()),
            ),
            WatchDetails::Store { .. } => ("store", None, None, None, None),
        };
        let id = match watch.details.as_store_row() {
            None => {
                sqlx::query!(
                    r#"
                        INSERT INTO watches (channel_id, kind, latitude, longitude, radius, regex)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                        RETURNING id AS "id!"
                    "#,
                    channel_id_str,
                    kind,
                    latitude,
                    longitude,
                    radius,
                    regex,
                )
                .fetch_one(&mut *conn)
                .await?
                .id
            }
            Some(store) => {
                sqlx::query!(
                    r#"
                        INSERT INTO watches (channel_id, kind, store_id, item_id, store_name)
                        VALUES (?1, ?2, ?3, ?4, ?5)
                        RETURNING id AS "id!"
                    "#,
                    channel_id_str,
                    kind,
                    store.store_id,
                    store.item_id,
                    store.store_name,
                )
                .fetch_one(&mut *conn)
                .await?
                .id
            }
        };
        Ok(id)
    }

    pub async fn remove_watch(&self, watch_id: i64) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query!(r#"DELETE FROM watches WHERE id = ?1"#, watch_id,)
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Re-links a delisted store watch to a fresh item of the same store.
    pub async fn update_item_id(&self, watch_id: i64, item_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query!(
            r#"UPDATE watches SET item_id = ?1 WHERE id = ?2"#,
            item_id,
            watch_id,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Marks a channel running/passive, creating the channel row if needed.
    pub async fn set_active(&self, channel_id: ChannelId, active: bool) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = channel_id.to_string();
        sqlx::query!(
            r#"
                INSERT INTO channels (channel_id, active) VALUES (?1, ?2)
                ON CONFLICT(channel_id) DO UPDATE SET active = excluded.active
            "#,
            channel_id_str,
            active,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn get_active(&self, channel_id: ChannelId) -> Result<bool> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = channel_id.to_string();
        let record = sqlx::query!(
            r#"
                SELECT active FROM channels WHERE channel_id = ?1
            "#,
            channel_id_str,
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(record.map(|r| r.active == 1).unwrap_or(false))
    }
}
