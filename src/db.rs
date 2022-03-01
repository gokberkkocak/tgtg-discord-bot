use std::{collections::{HashMap, HashSet}, str::FromStr};

use anyhow::Result;
use serenity::model::id::ChannelId;
use sqlx::SqlitePool;
use tracing::info;

use crate::CoordinatesWithRadius;

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

    pub async fn set_location(
        &self,
        channel_id: ChannelId,
        coords: CoordinatesWithRadius,
    ) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        let channel_id_str = channel_id.to_string();
        info!("chnl_id_str: {}", channel_id_str);
        let optional_rec = sqlx::query!(
            r#"
                SELECT active FROM channels WHERE channel_id = ?1
            "#,
            channel_id_str,
        )
        .fetch_optional(&mut conn)
        .await?;
        match optional_rec {
            Some(r) => {
                sqlx::query!(
                    r#"
                        UPDATE channels SET latitude = ?1, longitude = ?2, radius = ?3, active = ?4 WHERE channel_id = ?5
                    "#,
                    coords.latitude,
                    coords.longitude,
                    coords.radius,
                    r.active,
                    channel_id_str,
                )
                .execute(&mut conn)
                .await?;
            }
            None => {
                sqlx::query!(
                    r#"
                        INSERT INTO channels (channel_id, latitude, longitude, radius, active) VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                    channel_id_str,
                    coords.latitude,
                    coords.longitude,
                    coords.radius,
                    0,
                )
                .execute(&mut conn)
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
        .fetch_optional(&mut conn)
        .await?;
        if let Some(_) = optional_rec {
            sqlx::query!(
                r#"
                    UPDATE channels SET active = ?1 WHERE channel_id = ?2
                "#,
                active,
                channel_id_str,
            )
            .execute(&mut conn)
            .await?;
        }
        Ok(())
    }

    pub async fn get_locations(
        &self,
    ) -> Result<(
        HashMap<ChannelId, CoordinatesWithRadius>,
        HashSet<ChannelId>,
    )> {
        let mut conn = self.pool.acquire().await.unwrap();
        let records = sqlx::query!(
            r#"
                SELECT channel_id, latitude, longitude, radius, active FROM channels
            "#
        )
        .fetch_all(&mut conn)
        .await?;
        let location_map = records
            .iter()
            .map(|r| {
                let channel_id = ChannelId::from_str(&r.channel_id).unwrap();
                (
                    channel_id,
                    CoordinatesWithRadius {
                        latitude: r.latitude as f64,
                        longitude: r.longitude as f64,
                        radius: r.radius as u8,
                    },
                )
            })
            .collect();
        let active_set = records.iter().filter_map(|r| {
            if r.active == 1 {
                let channel_id = ChannelId::from_str(&r.channel_id).unwrap();
                Some(channel_id)
            } else {
                None
            }
        }).collect();
        Ok((location_map, active_set))
    }
}
