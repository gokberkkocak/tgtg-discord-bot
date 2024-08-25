use chrono::Utc;
use poise::serenity_prelude as serenity;
use serenity::builder::CreateEmbed;
use serenity::builder::CreateMessage;
use serenity::builder::EditMessage;
use serenity::prelude::RwLock;
use serenity::{http::Http, model::id::ChannelId};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use tracing::warn;

use crate::data::ItemMessage;
use crate::data::TGTGConfig;
use crate::data::OSM_ZOOM_LEVEL;
use crate::data::RADIUS_UNIT;
use crate::TGTGBindings;

const MONITOR_INTERVAL: u64 = 60;

pub async fn monitor_location(
    // client_data: Arc<RwLock<TypeMap>>,
    http: Arc<Http>,
    channel_id: ChannelId,
    active_channels: Arc<RwLock<HashSet<ChannelId>>>,
    tgtg_bindings: Arc<TGTGBindings>,
    tgtg_configs: Arc<RwLock<HashMap<ChannelId, TGTGConfig>>>,
    tgtg_messages: Arc<RwLock<HashMap<String, ItemMessage>>>,
) {
    let config = {
        let location_container = tgtg_configs.read().await;
        location_container
            .get(&channel_id)
            .expect("Channel not found")
            .clone()
    };

    tokio::spawn(async move {
        loop {
            // If stop command is called. Stop monitoring
            let stop_flag = {
                let exist_flag = active_channels.read().await.contains(&channel_id);
                !exist_flag
            };
            if stop_flag {
                break;
            }
            let res: Result<(), anyhow::Error> = update_location(
                tgtg_bindings.clone(),
                tgtg_messages.clone(),
                http.clone(),
                channel_id,
                &config,
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
    tgtg_bindings: Arc<TGTGBindings>,
    tgtg_messages: Arc<RwLock<HashMap<String, ItemMessage>>>,
    http: Arc<Http>,
    channel_id: ChannelId,
    config: &TGTGConfig,
) -> anyhow::Result<()> {
    let items = crate::tgtg::get_items(&tgtg_bindings, config)?;
    info!(
        "Channel {}: Monitor found {} items",
        channel_id,
        items.len()
    );
    let almost_now = Utc::now();
    for i in items {
        let item_message = {
            let item_map = tgtg_messages.read().await;
            item_map.get(&i.item.item_id).copied()
        };
        // check regex
        if let Some(regex) = config.regex.as_ref() {
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
            // Construct a new message embed with quantity and date to post or update
            let mut embed = CreateEmbed::new()
                .title(i.store.store_name)
                .description(i.display_name)
                .field(
                    "Price",
                    format!(
                        "{:.2} {}",
                        i.item.price_including_taxes.minor_units as f64
                            / 10u32.pow(i.item.price_including_taxes.decimals) as f64,
                        i.item.price_including_taxes.code
                    ),
                    true,
                )
                .field("Quantity", format!("{}", i.items_available), true)
                .field(
                    "Distance",
                    format!("{:.2} {}", i.distance, RADIUS_UNIT),
                    true,
                )
                .image(i.store.logo_picture.current_url)
                .url(format!(
                    "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                    OSM_ZOOM_LEVEL,
                    i.pickup_location.location.latitude,
                    i.pickup_location.location.longitude
                ));
            if let Some(interval) = i.pickup_interval {
                let timezone = i.store.store_time_zone;
                embed = embed.field(
                    "Pickup interval",
                    format!(
                        "{} - {}",
                        interval
                            .start
                            .with_timezone(&timezone)
                            .format("%a %H:%M %Z"),
                        interval.end.with_timezone(&timezone).format("%a %H:%M %Z")
                    ),
                    true,
                );
            }
            if let Some(item_message) = item_message {
                // Update the message with the new quantity
                if item_message.quantity != i.items_available {
                    let builder = EditMessage::new().embed(embed);
                    channel_id
                        .edit_message(&http, item_message.message_id, builder)
                        .await?;
                    let mut item_map = tgtg_messages.write().await;
                    item_map.insert(
                        i.item.item_id,
                        ItemMessage {
                            message_id: item_message.message_id,
                            quantity: i.items_available,
                            channel_id
                        },
                    );
                }
            } else {
                // We have quantity available, post a new message
                let builder = CreateMessage::new().add_embed(embed);
                let msg = channel_id.send_message(&http, builder).await?;
                let mut item_map_write = tgtg_messages.write().await;
                item_map_write.insert(
                    i.item.item_id,
                    ItemMessage {
                        message_id: msg.id,
                        quantity: i.items_available,
                        channel_id
                    },
                );
            }
        } else {
            // No quantity or purchase period has passed. Check we posted this item before, if yes delete
            if let Some(item_message) = item_message {
                channel_id
                    .delete_message(&http, item_message.message_id)
                    .await?;
                let mut item_map_write = tgtg_messages.write().await;
                item_map_write.remove(&i.item.item_id);
            }
        }
    }
    Ok(())
}
