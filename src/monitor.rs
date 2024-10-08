use chrono::Utc;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::futures::stream;
use poise::serenity_prelude::futures::StreamExt as _;
use serenity::builder::CreateEmbed;
use serenity::builder::CreateMessage;
use serenity::builder::EditMessage;
use serenity::prelude::RwLock;
use serenity::{http::Http, model::id::ChannelId};
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::info;
use tracing::warn;

use crate::data::ItemMessage;
use crate::data::TGTGConfig;
use crate::data::OSM_ZOOM_LEVEL;
use crate::data::RADIUS_UNIT;
use crate::TGTGBindings;

const MONITOR_INTERVAL: u64 = 60;

pub struct ChannelMonitor {
    pub channel_id: ChannelId,
    http: Arc<Http>,
    handle: JoinHandle<()>,
    messages: Arc<RwLock<HashMap<String, ItemMessage>>>,
}

impl ChannelMonitor {
    pub fn init(
        http: Arc<Http>,
        channel_id: ChannelId,
        tgtg_bindings: Arc<TGTGBindings>,
        tgtg_config: TGTGConfig,
    ) -> Self {
        info!("Channel {}: Monitor starting (DB) ", channel_id);
        let messages = Arc::new(RwLock::new(HashMap::new()));
        let loop_messages = messages.clone();
        let loop_http = http.clone();
        let handle = tokio::spawn(async move {
            loop {
                let res = ChannelMonitor::update_location(
                    tgtg_bindings.clone(),
                    loop_http.clone(),
                    channel_id,
                    tgtg_config.clone(),
                    loop_messages.clone(),
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
        });
        Self {
            channel_id,
            http,
            handle,
            messages,
        }
    }

    async fn update_location(
        tgtg_bindings: Arc<TGTGBindings>,
        http: Arc<Http>,
        channel_id: ChannelId,
        config: TGTGConfig,
        messages: Arc<RwLock<HashMap<String, ItemMessage>>>,
    ) -> anyhow::Result<()> {
        let items = crate::tgtg::get_items(&tgtg_bindings, &config)?;
        info!(
            "Channel {}: Monitor found {} items",
            channel_id,
            items.len()
        );
        let almost_now = Utc::now();
        for i in items {
            let item_message = {
                let item_map = messages.read().await;
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
                        let mut items_map = messages.write().await;
                        items_map.insert(
                            i.item.item_id,
                            ItemMessage {
                                message_id: item_message.message_id,
                                quantity: i.items_available,
                            },
                        );
                    }
                } else {
                    // We have quantity available, post a new message
                    let builder = CreateMessage::new().add_embed(embed);
                    let msg = channel_id.send_message(&http, builder).await?;
                    let mut items_map = messages.write().await;
                    items_map.insert(
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
                    let mut items_map = messages.write().await;
                    items_map.remove(&i.item.item_id);
                }
            }
        }
        Ok(())
    }
}

impl Drop for ChannelMonitor {
    fn drop(&mut self) {
        // abort watching
        self.handle.abort();
        // remove all messages from the discord channel
        let messages = self.messages.clone();
        let http = self.http.clone();
        let channel_id = self.channel_id;
        // block_in_place ensures waiting for the block to finish even the executor is shutting down
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let item_messages = messages.read().await;
                let count = stream::iter(item_messages.values())
                    .filter_map(|v| async {
                        channel_id
                            .delete_message(&http, v.message_id)
                            .await
                            .is_ok()
                            .then_some(())
                    })
                    .count()
                    .await;
                tracing::debug!(
                    "Channel {}: {} messages deleted from the discord channel",
                    channel_id, count
                );
            });
        });
        info!(
            "Channel {}: Task terminated for monitoring location",
            self.channel_id
        );
    }
}

impl Hash for ChannelMonitor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.channel_id.hash(state);
    }
}

impl PartialEq for ChannelMonitor {
    fn eq(&self, other: &Self) -> bool {
        self.channel_id == other.channel_id
    }
}

impl Eq for ChannelMonitor {}
