use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::futures::StreamExt as _;
use poise::serenity_prelude::futures::stream;
use regex::Regex;
use serenity::builder::{CreateEmbed, CreateMessage, EditMessage};
use serenity::http::Http;
use serenity::model::id::ChannelId;
use serenity::prelude::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::data::{
    BrowseOrigin, ItemFetchResult, ItemMessage, MAX_BROWSE_RADIUS, OSM_ZOOM_LEVEL, RADIUS_UNIT,
    TGTGBindings, TGTGListing, Watch, WatchDetails,
};

const MONITOR_INTERVAL: u64 = 60;
/// Consecutive genuine "item not found" results before a store watch is given up.
const MAX_STORE_MISSES: u32 = 10;
/// After this many consecutive misses a one-time relink attempt via search phrase is started.
const RELINK_AFTER_MISSES: u32 = 2;

pub type MessageKey = (i64, String);

pub type SharedMessages = Arc<RwLock<HashMap<MessageKey, ItemMessage>>>;
pub type SharedWatches = Arc<RwLock<HashMap<ChannelId, Vec<Watch>>>>;
pub type SharedLastFired = Arc<RwLock<HashMap<i64, DateTime<Utc>>>>;

/// Checks an optional regex filter against a listing name.
pub fn matches_filter(regex: &Option<Regex>, display_name: &str) -> bool {
    match regex {
        Some(regex) => regex.is_match(display_name),
        None => true,
    }
}

/// Returns the listing of a given store with the most items in stock.
/// Used to pin a store watch onto a concrete item of that store.
pub fn best_listing_for_store<'a>(
    listings: &'a [TGTGListing],
    store_id: &str,
) -> Option<&'a TGTGListing> {
    listings
        .iter()
        .filter(|l| l.store.store_id == store_id)
        .max_by_key(|l| l.items_available)
}

pub struct ChannelMonitor {
    pub channel_id: ChannelId,
    http: Arc<Http>,
    handle: JoinHandle<()>,
    messages: SharedMessages,
}

impl ChannelMonitor {
    pub fn init(
        http: Arc<Http>,
        channel_id: ChannelId,
        tgtg_bindings: Arc<TGTGBindings>,
        bot_db: Arc<crate::db::BotDB>,
        watches: SharedWatches,
        last_fired: SharedLastFired,
    ) -> Self {
        info!("Channel {}: Monitor starting", channel_id);
        let messages: SharedMessages = Arc::new(RwLock::new(HashMap::new()));
        let loop_messages = messages.clone();
        let loop_http = http.clone();
        let handle = tokio::spawn(async move {
            let mut miss_counters: HashMap<i64, u32> = HashMap::new();
            loop {
                let cycle_watches: Vec<Watch> = watches
                    .read()
                    .await
                    .get(&channel_id)
                    .cloned()
                    .unwrap_or_default();
                if cycle_watches.is_empty() {
                    info!("Channel {}: No watches configured, idling", channel_id);
                    tokio::time::sleep(Duration::from_secs(MONITOR_INTERVAL)).await;
                    continue;
                }
                for watch in &cycle_watches {
                    let res = ChannelMonitor::update_watch(
                        &loop_http,
                        &tgtg_bindings,
                        &bot_db,
                        &watches,
                        channel_id,
                        watch,
                        &mut miss_counters,
                        &loop_messages,
                        &last_fired,
                    )
                    .await;
                    if let Err(why) = res {
                        warn!(
                            "Channel {}: Failed to update watch {}: {}",
                            channel_id, watch.id, why
                        );
                    }
                }
                // Remove tracked messages of watches that are gone (unwatched or retired).
                let valid_ids: HashSet<i64> = watches
                    .read()
                    .await
                    .get(&channel_id)
                    .map(|v| v.iter().map(|w| w.id).collect())
                    .unwrap_or_default();
                ChannelMonitor::sweep_removed_watches(
                    &loop_http,
                    channel_id,
                    &valid_ids,
                    &loop_messages,
                )
                .await;
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

    #[allow(clippy::too_many_arguments)]
    async fn update_watch(
        http: &Arc<Http>,
        tgtg_bindings: &Arc<TGTGBindings>,
        bot_db: &Arc<crate::db::BotDB>,
        watches: &SharedWatches,
        channel_id: ChannelId,
        watch: &Watch,
        miss_counters: &mut HashMap<i64, u32>,
        messages: &SharedMessages,
        last_fired: &SharedLastFired,
    ) -> anyhow::Result<()> {
        match &watch.details {
            WatchDetails::Location {
                latitude,
                longitude,
                radius,
                regex,
            } => {
                let origin = BrowseOrigin {
                    latitude: *latitude,
                    longitude: *longitude,
                    radius: *radius,
                };
                let listings = crate::tgtg::browse_area(tgtg_bindings, &origin, None)?;
                info!(
                    "Channel {}: Location watch {} found {} shops",
                    channel_id,
                    watch.id,
                    listings.len()
                );
                let almost_now = Utc::now();
                for listing in listings {
                    if !matches_filter(regex, &listing.display_name) {
                        info!(
                            "Channel {}: Item {} with quantity {} - not matching filter",
                            channel_id, listing.display_name, listing.items_available
                        );
                        continue;
                    }
                    ChannelMonitor::handle_listing(
                        http, channel_id, watch, listing, almost_now, messages, last_fired,
                    )
                    .await?;
                }
                Ok(())
            }
            WatchDetails::Store {
                item_id,
                store_name,
                ..
            } => match crate::tgtg::get_item(tgtg_bindings, item_id)? {
                ItemFetchResult::Found(listing) => {
                    miss_counters.remove(&watch.id);
                    let almost_now = Utc::now();
                    ChannelMonitor::handle_listing(
                        http, channel_id, watch, *listing, almost_now, messages, last_fired,
                    )
                    .await
                }
                ItemFetchResult::NotFound => {
                    let misses = *miss_counters
                        .entry(watch.id)
                        .and_modify(|m| *m += 1)
                        .or_insert(1);
                    warn!(
                        "Channel {}: Store {} not found (miss {}/{} of watch {})",
                        channel_id, store_name, misses, MAX_STORE_MISSES, watch.id
                    );
                    if misses == RELINK_AFTER_MISSES
                        && let Some(relisted) =
                            ChannelMonitor::relink_store(tgtg_bindings, bot_db, watches, watch)
                                .await?
                    {
                        info!(
                            "Channel {}: Relinked store watch {} to item {}",
                            channel_id, watch.id, relisted.item.item_id
                        );
                        miss_counters.remove(&watch.id);
                    }
                    if misses >= MAX_STORE_MISSES {
                        ChannelMonitor::retire_store_watch(
                            http, bot_db, watches, channel_id, watch, store_name,
                        )
                        .await?;
                        miss_counters.remove(&watch.id);
                        last_fired.write().await.remove(&watch.id);
                    }
                    Ok(())
                }
            },
        }
    }

    /// Posts, updates or deletes the channel message of a single listing
    /// according to its current availability.
    async fn handle_listing(
        http: &Arc<Http>,
        channel_id: ChannelId,
        watch: &Watch,
        listing: TGTGListing,
        almost_now: DateTime<Utc>,
        messages: &SharedMessages,
        last_fired: &SharedLastFired,
    ) -> anyhow::Result<()> {
        let key: MessageKey = (watch.id, listing.item.item_id.clone());
        let item_message = messages.read().await.get(&key).copied();
        info!(
            "Channel {}: Item {} of watch {} with quantity {} - matching filter",
            channel_id, listing.display_name, watch.id, listing.items_available
        );
        // Check if the item is available and if we are in the purchase time period
        if listing.purchase_end.map(|end| end > almost_now).is_some() && listing.items_available > 0
        {
            let embed = build_embed(&listing);
            match item_message {
                Some(item_message) => {
                    // Update the message with the new quantity
                    if item_message.quantity != listing.items_available {
                        let builder = EditMessage::new().embed(embed);
                        channel_id
                            .edit_message(http, item_message.message_id, builder)
                            .await?;
                        messages.write().await.insert(
                            key,
                            ItemMessage {
                                message_id: item_message.message_id,
                                quantity: listing.items_available,
                            },
                        );
                    }
                }
                None => {
                    // We have quantity available, post a new message
                    let builder = CreateMessage::new().add_embed(embed);
                    let msg = channel_id.send_message(http, builder).await?;
                    last_fired.write().await.insert(watch.id, almost_now);
                    messages.write().await.insert(
                        key,
                        ItemMessage {
                            message_id: msg.id,
                            quantity: listing.items_available,
                        },
                    );
                }
            }
        } else {
            // No quantity or purchase period has passed. Check we posted this item before, if yes delete
            if let Some(item_message) = item_message {
                channel_id
                    .delete_message(http, item_message.message_id)
                    .await?;
                messages.write().await.remove(&key);
            }
        }
        Ok(())
    }

    /// Attempts to re-link a delisted store watch to a fresh item of the same
    /// store by searching around its stored coordinates once.
    async fn relink_store(
        tgtg_bindings: &Arc<TGTGBindings>,
        bot_db: &Arc<crate::db::BotDB>,
        watches: &SharedWatches,
        watch: &Watch,
    ) -> anyhow::Result<Option<TGTGListing>> {
        let WatchDetails::Store {
            store_id,
            store_name,
            latitude,
            longitude,
            ..
        } = &watch.details
        else {
            return Ok(None);
        };
        let origin = BrowseOrigin {
            latitude: *latitude,
            longitude: *longitude,
            radius: MAX_BROWSE_RADIUS,
        };
        let listings = crate::tgtg::browse_area(tgtg_bindings, &origin, Some(store_name))?;
        let relisted = best_listing_for_store(&listings, store_id).cloned();
        if let Some(listing) = &relisted {
            bot_db
                .update_item_id(watch.id, &listing.item.item_id)
                .await?;
            if let Some(WatchDetails::Store { item_id, .. }) = watches
                .write()
                .await
                .get_mut(&watch.channel_id)
                .and_then(|channel_watches| {
                    channel_watches
                        .iter_mut()
                        .find(|w| w.id == watch.id)
                        .map(|current| &mut current.details)
                })
            {
                *item_id = listing.item.item_id.clone();
            }
        }
        Ok(relisted)
    }

    /// Removes a dead store watch from state and database and notifies the channel.
    async fn retire_store_watch(
        http: &Arc<Http>,
        bot_db: &Arc<crate::db::BotDB>,
        watches: &SharedWatches,
        channel_id: ChannelId,
        watch: &Watch,
        store_name: &str,
    ) -> anyhow::Result<()> {
        info!(
            "Channel {}: Removing store watch {} for store {}",
            channel_id, watch.id, store_name
        );
        bot_db.remove_watch(watch.id).await?;
        if let Some(channel_watches) = watches.write().await.get_mut(&channel_id) {
            channel_watches.retain(|w| w.id != watch.id);
        }
        // Its remaining messages are cleaned up by the end-of-cycle sweep.
        let embed = CreateEmbed::new()
            .title("\u{1F6D1} Watch removed")
            .description(format!(
                "The store **{}** seems to be no longer available on TooGoodToGo. Its watch has been removed.",
                store_name
            ));
        let builder = CreateMessage::new().add_embed(embed);
        channel_id.send_message(http, builder).await?;
        Ok(())
    }

    async fn sweep_removed_watches(
        http: &Arc<Http>,
        channel_id: ChannelId,
        valid_ids: &HashSet<i64>,
        messages: &SharedMessages,
    ) {
        let to_delete: Vec<MessageKey> = messages
            .read()
            .await
            .keys()
            .filter(|key| !valid_ids.contains(&key.0))
            .cloned()
            .collect();
        for key in to_delete {
            let message = messages.read().await.get(&key).copied();
            if let Some(message) = message {
                if let Err(why) = channel_id.delete_message(http, message.message_id).await {
                    warn!(
                        "Channel {}: Could not delete message of removed watch {}: {}",
                        channel_id, key.0, why
                    );
                }
                messages.write().await.remove(&key);
            }
        }
    }
}

fn build_embed(listing: &TGTGListing) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(&listing.store.store_name)
        .description(&listing.display_name)
        .field(
            "Price",
            format!(
                "{:.2} {}",
                listing.item.price_including_taxes.minor_units as f64
                    / 10u32.pow(listing.item.price_including_taxes.decimals) as f64,
                listing.item.price_including_taxes.code
            ),
            true,
        )
        .field("Quantity", format!("{}", listing.items_available), true);
    if let Some(distance) = listing.distance {
        embed = embed.field("Distance", format!("{:.2} {}", distance, RADIUS_UNIT), true);
    }
    embed = embed.image(listing.store.logo_picture.current_url.clone());
    if let Some(pickup_location) = &listing.pickup_location {
        embed = embed.url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, pickup_location.location.latitude, pickup_location.location.longitude
        ));
    }
    if let Some(interval) = &listing.pickup_interval {
        let timezone = listing.store.store_time_zone;
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
    embed
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
                    channel_id,
                    count
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

#[cfg(test)]
mod test {
    use super::*;

    fn listing_json(store_id: &str, name: &str, quantity: usize) -> String {
        format!(
            r#"{{
                "item": {{
                    "item_id": "item-{store_id}",
                    "price_including_taxes": {{"code": "EUR", "minor_units": 399, "decimals": 2}}
                }},
                "store": {{
                    "store_id": "{store_id}",
                    "store_name": "{name}",
                    "logo_picture": {{"current_url": "https://example.com/logo.png"}},
                    "store_time_zone": "Europe/Berlin"
                }},
                "display_name": "{name} - Surprise Bag",
                "items_available": {quantity},
                "distance": 1.5,
                "pickup_interval": null,
                "purchase_end": null
            }}"#
        )
    }

    #[test]
    fn test_matches_filter() {
        let none: Option<Regex> = None;
        assert!(matches_filter(&none, "Everything matches"));
        let pizza = Regex::new("(?i)pizza").unwrap();
        assert!(matches_filter(&Some(pizza.clone()), "Nice Pizza Place"));
        assert!(!matches_filter(&Some(pizza), "Burger Joint"));
    }

    #[test]
    fn test_best_listing_for_store_prefers_most_stock() {
        let listings: Vec<TGTGListing> = [
            listing_json("s1", "Alpha", 0),
            listing_json("s2", "Beta", 1),
            listing_json("s1", "Alpha XL", 3),
            listing_json("s1", "Alpha Mini", 2),
        ]
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
        let best = best_listing_for_store(&listings, "s1").unwrap();
        assert_eq!(best.store.store_id, "s1");
        assert_eq!(best.display_name, "Alpha XL - Surprise Bag");
        assert_eq!(best.items_available, 3);
        assert!(best_listing_for_store(&listings, "missing").is_none());
    }

    #[test]
    fn test_listing_deserialization_requires_store_id() {
        let listing: TGTGListing = serde_json::from_str(&listing_json("s9", "Gamma", 7)).unwrap();
        assert_eq!(listing.store.store_id, "s9");
        assert_eq!(listing.items_available, 7);
    }
}
