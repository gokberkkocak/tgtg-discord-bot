use anyhow::Context;
use chrono::Utc;
use serenity::prelude::RwLock;
use serenity::prelude::TypeMap;
use serenity::{http::Http, model::id::ChannelId};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use tracing::warn;

use crate::TGTGActiveChannelsContainer;
use crate::TGTGCredentials;
use crate::RADIUS_UNIT;
use crate::{
    CoordinatesWithRadius, ItemMessage, TGTGCredentialsContainer, TGTGItemMessageContainer,
};

const MONITOR_INTERVAL: u64 = 60;

pub async fn monitor_location(
    client_data: Arc<RwLock<TypeMap>>,
    http: Arc<Http>,
    channel_id: ChannelId,
    coords: CoordinatesWithRadius,
) {
    let tgtg_credentials = {
        let client_data = client_data.read().await;
        client_data
            .get::<TGTGCredentialsContainer>()
            .expect("Credentials missing")
            .clone()
    };
    tokio::spawn(async move {
        loop {
            // If stop command is called. Stop monitoring
            let stop_flag = {
                let client_data = client_data.read().await;
                let exist_flag = client_data
                    .get::<TGTGActiveChannelsContainer>()
                    .expect("Active channels missing")
                    .read()
                    .await
                    .contains(&channel_id);
                !exist_flag
            };
            if stop_flag {
                break;
            }
            let res = update_location(
                tgtg_credentials.clone(),
                client_data.clone(),
                http.clone(),
                channel_id,
                coords.clone(),
            )
            .await;
            if let Err(why) = res {
                warn!(
                    "Channel {}: Failed to update location with {}",
                    channel_id, why
                );
            }
            tokio::time::sleep(Duration::from_secs(MONITOR_INTERVAL)).await;
        }
        info!(
            "Channel {}: Thread terminated for monitoring location",
            channel_id
        );
    });
}

async fn update_location(
    tgtg_credentials: Arc<TGTGCredentials>,
    client_data: Arc<RwLock<TypeMap>>,
    http: Arc<Http>,
    channel_id: ChannelId,
    coords: CoordinatesWithRadius,
) -> anyhow::Result<()> {
    let client_data_rw = client_data.write().await;
    let items = crate::tgtg::get_items(&tgtg_credentials, &coords)?;
    info!(
        "Channel {}: Monitor found {} items",
        channel_id,
        items.len()
    );
    let almost_now = Utc::now();
    for i in items {
        let item_message = {
            let item_map = client_data_rw
                .get::<TGTGItemMessageContainer>()
                .context("Item message missing")?
                .read()
                .await;
            item_map.get(&i.item.item_id).copied()
        };
        // check regex
        if let Some(regex) = coords.regex.as_ref() {
            if !regex.is_match(&i.display_name) {
                info!(
                    "Channel {}: Item {} with quantity {} - not matching regex",
                    channel_id, i.display_name, i.items_available
                );
                continue;
            }
        }
        info!(
            "Channel {}: Item {} with quantity {} - matching regex",
            channel_id, i.display_name, i.items_available
        );
        //  Check if the item is available and if we are in the purchase time period
        if i.purchase_end
            .map(|end_time| end_time > almost_now)
            .is_some()
            && i.items_available > 0
        {
            if let Some(item_message) = item_message {
                // Update the message with the new quantity
                if item_message.quantity != i.items_available {
                    channel_id
                        .edit_message(&http, item_message.message_id, |m| {
                            m.embed(|e| {
                                e.title(i.store.store_name);
                                e.description(i.display_name);
                                e.field(
                                    "Price",
                                    format!(
                                        "{:.2} {}",
                                        i.item.price_including_taxes.minor_units as f64
                                            / 10u32.pow(i.item.price_including_taxes.decimals)
                                                as f64,
                                        i.item.price_including_taxes.code
                                    ),
                                    true,
                                );
                                e.field("Quantity", i.items_available, true);
                                if let Some(interval) = i.pickup_interval {
                                    e.field(
                                        "Pickup interval",
                                        format!(
                                            "{} - {}",
                                            interval.start.format("%a %H:%M"),
                                            interval.end.format("%a %H:%M")
                                        ),
                                        true,
                                    );
                                }
                                e.field(
                                    "Distance",
                                    format!("{:.2} {}", i.distance, RADIUS_UNIT),
                                    true,
                                );
                                e.image(i.store.logo_picture.current_url);
                                e
                            });
                            m
                        })
                        .await?;
                    let mut item_map = client_data_rw
                        .get::<TGTGItemMessageContainer>()
                        .context("Item message missing")?
                        .write()
                        .await;
                    item_map.insert(
                        i.item.item_id,
                        ItemMessage {
                            message_id: item_message.message_id,
                            quantity: i.items_available,
                        },
                    );
                }
            } else {
                // We have quantity available, post a new message
                let msg = channel_id
                    .send_message(&http, |m| {
                        m.embed(|e| {
                            e.title(i.store.store_name);
                            e.description(i.display_name);
                            e.field(
                                "Price",
                                format!(
                                    "{:.2} {}",
                                    i.item.price_including_taxes.minor_units as f64
                                        / 10u32.pow(i.item.price_including_taxes.decimals) as f64,
                                    i.item.price_including_taxes.code
                                ),
                                true,
                            );
                            e.field("Quantity", i.items_available, true);
                            if let Some(interval) = i.pickup_interval {
                                e.field(
                                    "Pickup interval",
                                    format!(
                                        "{} - {}",
                                        interval.start.format("%a %H:%M"),
                                        interval.end.format("%a %H:%M")
                                    ),
                                    true,
                                );
                            }
                            e.field(
                                "Distance",
                                format!("{:.2} {}", i.distance, RADIUS_UNIT),
                                true,
                            );
                            e.image(i.store.logo_picture.current_url);
                            e
                        });
                        m
                    })
                    .await?;
                let mut item_map_write = client_data_rw
                    .get::<TGTGItemMessageContainer>()
                    .context("Item message missing")?
                    .write()
                    .await;
                item_map_write.insert(
                    i.item.item_id,
                    ItemMessage {
                        message_id: msg.id,
                        quantity: i.items_available,
                    },
                );
            }
        } else {
            // No quantity or purchase period has passed. Check we posted this item before, if yes delete
            if let Some(item_message) = item_message {
                channel_id
                    .delete_message(&http, item_message.message_id)
                    .await?;
                let mut item_map_write = client_data_rw
                    .get::<TGTGItemMessageContainer>()
                    .context("Item message missing")?
                    .write()
                    .await;
                item_map_write.remove(&i.item.item_id);
            }
        }
    }
    // manually drop lock. we don't want to keep the lock during the sleeping period.
    drop(client_data_rw);
    Ok(())
}
