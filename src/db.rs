use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use anyhow::Result;
use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::model::id::ChannelId;
use sqlx::SqlitePool;

use crate::data::TGTGConfig;

pub struct BotDB {
    pool: SqlitePool,
}

impl BotDB {
    pub async fn new(db_url: &str) -> Result<Self> {
        let token_db = BotDB {
            pool: SqlitePool::connect(db_url).await?,
        };
        Ok(token_db)
    }

    pub async fn set_location(&self, channel_id: ChannelId, config: &TGTGConfig) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = channel_id.to_string();
        let optional_rec = sqlx::query!(
            r#"
                SELECT active FROM channels WHERE channel_id = ?1
            "#,
            channel_id_str,
        )
        .fetch_optional(&mut *conn)
        .await?;
        let regex_str = config.regex.as_ref().map(|r| r.as_str());
        match optional_rec {
            Some(r) => {
                sqlx::query!(
                    r#"
                        UPDATE channels SET latitude = ?1, longitude = ?2, radius = ?3, regex = ?4, active = ?5 WHERE channel_id = ?6
                    "#,
                    config.latitude,
                    config.longitude,
                    config.radius,
                    regex_str,
                    r.active,
                    channel_id_str,
                )
                .execute(&mut *conn)
                .await?;
            }
            None => {
                sqlx::query!(
                    r#"
                        INSERT INTO channels (channel_id, latitude, longitude, radius, regex, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                    channel_id_str,
                    config.latitude,
                    config.longitude,
                    config.radius,
                    regex_str,
                    0,
                )
                .execute(&mut *conn)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn change_active(&self, channel_id: ChannelId, active: bool) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = channel_id.to_string();
        let optional_rec = sqlx::query!(
            r#"
                SELECT active FROM channels WHERE channel_id = ?1
            "#,
            channel_id_str,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if optional_rec.is_some() {
            sqlx::query!(
                r#"
                    UPDATE channels SET active = ?1 WHERE channel_id = ?2
                "#,
                active,
                channel_id_str,
            )
            .execute(&mut *conn)
            .await?;
        }
        Ok(())
    }

    pub async fn get_locations(
        &self,
    ) -> Result<(HashMap<ChannelId, TGTGConfig>, HashSet<ChannelId>)> {
        let mut conn = self.pool.acquire().await?;
        let records = sqlx::query!(
            r#"
                SELECT channel_id, latitude, longitude, radius, regex, active FROM channels
            "#
        )
        .fetch_all(&mut *conn)
        .await?;
        let location_map = records
            .iter()
            .map(|r| {
                let channel_id = ChannelId::from_str(&r.channel_id).expect("Invalid channel id");
                let mut config =
                    TGTGConfig::new_with_radius(r.latitude, r.longitude, r.radius as u8);
                if let Some(regex_str) = &r.regex {
                    config.regex = Some(Regex::new(regex_str).expect("Invalid regex"));
                }
                (channel_id, config)
            })
            .collect();
        let active_set = records
            .iter()
            .filter_map(|r| {
                if r.active == 1 {
                    let channel_id =
                        ChannelId::from_str(&r.channel_id).expect("Invalid channel id");
                    Some(channel_id)
                } else {
                    None
                }
            })
            .collect();
        Ok((location_map, active_set))
    }
}
