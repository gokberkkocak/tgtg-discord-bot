use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use poise::serenity_prelude as serenity;
use regex::Regex;

use serde::Deserialize;
use serenity::all::{ChannelId, MessageId};
use tokio::sync::RwLock;

use crate::monitor::ChannelMonitor;

pub static RADIUS_UNIT: &str = "km";
pub static DEFAULT_RADIUS: u8 = 1;
pub static DEFAULT_BROWSE_RADIUS: u8 = 3;
pub static MAX_BROWSE_RADIUS: u8 = 21;
pub static OSM_ZOOM_LEVEL: u8 = 15;
pub static LIST_PAGE_SIZE: usize = 10;

#[derive(Clone, Copy)]
pub struct ItemMessage {
    pub message_id: MessageId,
    pub quantity: usize,
}

/// Origin of a browse/lookup area, remembered from the last `/list-shops` usage.
#[derive(Clone, Copy, Debug)]
pub struct BrowseOrigin {
    pub latitude: f64,
    pub longitude: f64,
    pub radius: u8,
}

/// A single subscription entry of a channel. Either an "everything in region"
/// location watch or a per-store watch pinned to a single item of one shop.
#[derive(Clone, Debug)]
pub struct Watch {
    pub id: i64,
    pub channel_id: ChannelId,
    pub details: WatchDetails,
}

#[derive(Clone, Debug)]
pub enum WatchDetails {
    Location {
        latitude: f64,
        longitude: f64,
        radius: u8,
        regex: Option<Regex>,
    },
    Store {
        store_id: String,
        item_id: String,
        store_name: String,
        latitude: f64,
        longitude: f64,
    },
}

impl Watch {
    pub fn new_location(
        channel_id: ChannelId,
        latitude: f64,
        longitude: f64,
        radius: u8,
        regex: Option<Regex>,
    ) -> Self {
        Self {
            id: 0,
            channel_id,
            details: WatchDetails::Location {
                latitude,
                longitude,
                radius,
                regex,
            },
        }
    }

    pub fn new_store(
        channel_id: ChannelId,
        store_id: String,
        item_id: String,
        store_name: String,
        latitude: f64,
        longitude: f64,
    ) -> Self {
        Self {
            id: 0,
            channel_id,
            details: WatchDetails::Store {
                store_id,
                item_id,
                store_name,
                latitude,
                longitude,
            },
        }
    }

    /// Short human readable parameter summary for listings.
    pub fn describe(&self) -> String {
        match &self.details {
            WatchDetails::Location {
                latitude,
                longitude,
                radius,
                regex,
            } => match regex {
                Some(regex) => format!(
                    "\u{1F4CD} location ({:.4}, {:.4}) r={} {} filter `{}`",
                    latitude,
                    longitude,
                    radius,
                    RADIUS_UNIT,
                    regex.as_str().replace('*', "\\*")
                ),
                None => format!(
                    "\u{1F4CD} location ({:.4}, {:.4}) r={} {}",
                    latitude, longitude, radius, RADIUS_UNIT
                ),
            },
            WatchDetails::Store { store_name, .. } => {
                format!("\u{1F3EA} store **{}**", store_name)
            }
        }
    }
}

/// Result of a single-item fetch: either the current listing or a signal that
/// the item is genuinely gone (not merely a transient failure).
#[derive(Debug)]
pub enum ItemFetchResult {
    Found(Box<TGTGListing>),
    NotFound,
}

#[derive(Debug)]
pub struct TGTGBindings {
    pub client: pyo3::Py<pyo3::PyAny>,
    pub fetch_func: pyo3::Py<pyo3::PyAny>,
    pub item_fetch_func: pyo3::Py<pyo3::PyAny>,
}

pub struct DiscordData {
    pub bot_db: Arc<crate::db::BotDB>,
    pub active_channels: Arc<RwLock<HashSet<ChannelMonitor>>>,
    pub tgtg_bindings: Arc<TGTGBindings>,
    /// Live watch state, re-read by monitors every cycle so commands take effect immediately.
    pub watches: Arc<RwLock<HashMap<ChannelId, Vec<Watch>>>>,
    /// Last browsed area origin per channel, used as lookup fallback for store watches.
    pub browse_origins: Arc<RwLock<HashMap<ChannelId, BrowseOrigin>>>,
    /// Last time a watch fired (posted a new listing message), keyed by watch id.
    pub last_fired: Arc<RwLock<HashMap<i64, DateTime<Utc>>>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TGTGListing {
    pub item: Item,
    pub store: Store,
    pub display_name: String,
    pub items_available: usize,
    pub distance: Option<f64>,
    pub pickup_location: Option<PickupLocation>,
    pub pickup_interval: Option<PickupInterval>,
    pub purchase_end: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub item_id: String,
    pub price_including_taxes: ItemPrice,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ItemPrice {
    pub code: String,
    pub minor_units: u32,
    pub decimals: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Store {
    pub store_id: String,
    pub store_name: String,
    pub logo_picture: Logo,
    pub store_time_zone: Tz,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Logo {
    pub current_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PickupInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PickupLocation {
    pub location: Location,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}
