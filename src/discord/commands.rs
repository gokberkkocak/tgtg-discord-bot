use anyhow::{Context as _, Result};
use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::all::CreateEmbed;
use tracing::info;

use crate::data::{
    BrowseOrigin, DEFAULT_BROWSE_RADIUS, DEFAULT_RADIUS, LIST_PAGE_SIZE, MAX_BROWSE_RADIUS,
    OSM_ZOOM_LEVEL, RADIUS_UNIT, Watch, WatchDetails,
};
use crate::monitor;

use super::{Context, Error};

/// Check the bot if it's ready to work
#[poise::command[prefix_command, slash_command]]
pub async fn health(ctx: Context<'_>) -> Result<(), Error> {
    info!("Channel {}: Health check recieved.", ctx.channel_id());
    ctx.say("I'm alive and healthy!").await?;
    Ok(())
}

/// Manage your shop and location watches
#[poise::command(prefix_command, slash_command, subcommands("location", "store"))]
pub async fn watch(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Use `/watch location` or `/watch store`.").await?;
    Ok(())
}

/// Watch all shops in an area around given coordinates
#[poise::command[prefix_command, slash_command]]
async fn location(
    ctx: Context<'_>,
    #[description = "latitude"] latitude: f64,
    #[description = "longitude"] longitude: f64,
    #[description = "radius in km (default 1)"] radius: Option<u8>,
    #[description = "regex filter for shop names"] regex: Option<String>,
) -> Result<(), Error> {
    let regex = match regex {
        Some(pattern) => Some(Regex::new(&pattern)?),
        None => None,
    };
    let radius = radius.unwrap_or(DEFAULT_RADIUS);
    let mut watch = Watch::new_location(ctx.channel_id(), latitude, longitude, radius, regex);
    let id = ctx.data().bot_db.add_watch(&watch).await?;
    watch.id = id;
    ctx.data()
        .watches
        .write()
        .await
        .entry(ctx.channel_id())
        .or_default()
        .push(watch.clone());
    info!("Channel {}: Location watch {} added", ctx.channel_id(), id);
    let embed = CreateEmbed::new()
        .title(format!("Watch #{} added", id))
        .description(watch.describe())
        .url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, latitude, longitude
        ))
        .field(
            "Status",
            if ctx.data().bot_db.get_active(ctx.channel_id()).await? {
                "\u{2705} running"
            } else {
                "\u{23F8} use /start"
            },
            true,
        );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Watch a single shop by its name (searched around a lookup origin)
#[poise::command[prefix_command, slash_command]]
async fn store(
    ctx: Context<'_>,
    #[description = "shop name to search"] name: String,
    #[description = "latitude of the lookup origin"] latitude: Option<f64>,
    #[description = "longitude of the lookup origin"] longitude: Option<f64>,
) -> Result<(), Error> {
    let origin = match resolve_origin(ctx, latitude, longitude, None)
        .await
        .context("lookup origin resolution")
    {
        Ok(origin) => origin,
        Err(why) => {
            ctx.say(format!(
                "{} Consider using /list-shops first or adding a location watch.",
                why
            ))
            .await?;
            return Ok(());
        }
    };
    let listings = crate::tgtg::browse_area(&ctx.data().tgtg_bindings, &origin, Some(&name))?;
    // Results are sorted by distance; pin the watch onto the best item of the nearest matching shop.
    let target_store_id = listings.first().map(|l| l.store.store_id.clone());
    let listing = target_store_id
        .as_deref()
        .and_then(|store_id| monitor::best_listing_for_store(&listings, store_id));
    let Some(listing) = listing else {
        ctx.say(format!(
            "No shop found for '{}' around ({:.4}, {:.4}).",
            name, origin.latitude, origin.longitude
        ))
        .await?;
        return Ok(());
    };
    let mut watch = Watch::new_store(
        ctx.channel_id(),
        listing.store.store_id.clone(),
        listing.item.item_id.clone(),
        listing.store.store_name.clone(),
        origin.latitude,
        origin.longitude,
    );
    let id = ctx.data().bot_db.add_watch(&watch).await?;
    watch.id = id;
    ctx.data()
        .watches
        .write()
        .await
        .entry(ctx.channel_id())
        .or_default()
        .push(watch.clone());
    info!(
        "Channel {}: Store watch {} added for {}",
        ctx.channel_id(),
        id,
        listing.store.store_name
    );
    let embed = CreateEmbed::new()
        .title(format!("Watch #{} added", id))
        .description(watch.describe())
        .field("Item", listing.display_name.clone(), true)
        .field("Quantity", format!("{}", listing.items_available), true)
        .field(
            "Status",
            if ctx.data().bot_db.get_active(ctx.channel_id()).await? {
                "\u{2705} running"
            } else {
                "\u{23F8} use /start"
            },
            true,
        );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Remove one of this channel's watches by its ID
#[poise::command[prefix_command, slash_command]]
pub async fn unwatch(
    ctx: Context<'_>,
    #[description = "watch ID (see /subscriptions)"] watch_id: i64,
) -> Result<(), Error> {
    let removed = ctx
        .data()
        .watches
        .write()
        .await
        .get_mut(&ctx.channel_id())
        .map(|channel_watches| {
            let len_before = channel_watches.len();
            channel_watches.retain(|w| w.id != watch_id);
            channel_watches.len() != len_before
        })
        .unwrap_or(false);
    if !removed {
        ctx.say(format!("No watch with ID {} in this channel.", watch_id))
            .await?;
        return Ok(());
    }
    ctx.data().last_fired.write().await.remove(&watch_id);
    ctx.data().bot_db.remove_watch(watch_id).await?;
    info!("Channel {}: Watch {} removed", ctx.channel_id(), watch_id);
    ctx.say(format!("Removed watch #{}.", watch_id)).await?;
    Ok(())
}

/// Browse shops of an area, including ones without stock
#[poise::command[prefix_command, slash_command]]
pub async fn list_shops(
    ctx: Context<'_>,
    #[description = "latitude"] latitude: Option<f64>,
    #[description = "longitude"] longitude: Option<f64>,
    #[description = "radius in km"] radius: Option<u8>,
    #[description = "substring filter for shop names"] filter: Option<String>,
    #[description = "page number"] page: Option<u32>,
) -> Result<(), Error> {
    let radius = radius.map(|r| r.min(MAX_BROWSE_RADIUS));
    let origin = match resolve_origin(ctx, latitude, longitude, radius)
        .await
        .context("lookup origin resolution")
    {
        Ok(origin) => origin,
        Err(why) => {
            ctx.say(format!(
                "{} Consider using coordinates or adding a location watch.",
                why
            ))
            .await?;
            return Ok(());
        }
    };
    // Remember the browsed area as future lookup origin for this channel.
    ctx.data()
        .browse_origins
        .write()
        .await
        .insert(ctx.channel_id(), origin);

    let listings = crate::tgtg::browse_area(&ctx.data().tgtg_bindings, &origin, None)?;
    let filter_lower = filter.as_deref().map(str::to_lowercase);
    let filtered: Vec<_> = listings
        .into_iter()
        .filter(|listing| match &filter_lower {
            Some(filter_lower) => listing
                .store
                .store_name
                .to_lowercase()
                .contains(filter_lower),
            None => true,
        })
        .collect();
    if filtered.is_empty() {
        ctx.say("No shops found for this area and filter.").await?;
        return Ok(());
    }

    let watched_stores: std::collections::HashSet<String> = ctx
        .data()
        .watches
        .read()
        .await
        .get(&ctx.channel_id())
        .map(|channel_watches| {
            channel_watches
                .iter()
                .filter_map(|w| match &w.details {
                    WatchDetails::Store { store_id, .. } => Some(store_id.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let total_pages = filtered.len().div_ceil(LIST_PAGE_SIZE).max(1) as u32;
    let page = page.unwrap_or(1).clamp(1, total_pages);
    let page_items: Vec<_> = filtered
        .iter()
        .skip((page as usize - 1) * LIST_PAGE_SIZE)
        .take(LIST_PAGE_SIZE)
        .collect();

    let mut description = String::new();
    for listing in page_items {
        let mark = if watched_stores.contains(&listing.store.store_id) {
            "\u{2705}"
        } else {
            "\u{25AB}"
        };
        let distance = listing
            .distance
            .map(|d| format!(" \u{00B7} {:.2} {}", d, RADIUS_UNIT))
            .unwrap_or_default();
        description.push_str(&format!(
            "{} **{}** \u{00B7} {} left{}\n",
            mark, listing.store.store_name, listing.items_available, distance
        ));
    }
    let embed = CreateEmbed::new()
        .title("Shops")
        .description(description)
        .footer(serenity::all::CreateEmbedFooter::new(match page {
            _ if page < total_pages => format!(
                "{} shops \u{00B7} page {} of {} \u{00B7} next: /list-shops page:{}",
                filtered.len(),
                page,
                total_pages,
                page + 1
            ),
            _ => format!(
                "{} shops \u{00B7} page {} of {}",
                filtered.len(),
                page,
                total_pages
            ),
        }))
        .url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, origin.latitude, origin.longitude
        ));
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show all watches of this channel with their current status
#[poise::command[prefix_command, slash_command]]
pub async fn subscriptions(ctx: Context<'_>) -> Result<(), Error> {
    let channel_watches = ctx
        .data()
        .watches
        .read()
        .await
        .get(&ctx.channel_id())
        .cloned();
    let Some(channel_watches) = channel_watches else {
        ctx.say("No watches yet! Use /watch to add some.").await?;
        return Ok(());
    };
    let running = ctx.data().bot_db.get_active(ctx.channel_id()).await?;
    let last_fired = ctx.data().last_fired.read().await;
    let mut description = String::new();
    for watch in &channel_watches {
        let status = if running { "\u{2705}" } else { "\u{23F8}" };
        let fired = match last_fired.get(&watch.id) {
            Some(fired) => fired.format("%Y-%m-%d %H:%M UTC").to_string(),
            None => "never".to_string(),
        };
        description.push_str(&format!(
            "`#{}` {} \u{00B7} {} \u{00B7} fired: {}\n",
            watch.id,
            watch.describe(),
            status,
            fired
        ));
    }
    let embed = CreateEmbed::new()
        .title("Subscriptions")
        .description(description)
        .field(
            "Monitor",
            if running {
                "\u{2705} running".to_string()
            } else {
                "\u{23F8} stopped".to_string()
            },
            true,
        );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Start monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    // Are we already monitoring
    if ctx
        .data()
        .active_channels
        .read()
        .await
        .iter()
        .any(|c| c.channel_id == ctx.channel_id())
    {
        ctx.reply("Already monitoring!").await?;
        return Ok(());
    }
    let watch_count = ctx
        .data()
        .watches
        .read()
        .await
        .get(&ctx.channel_id())
        .map(Vec::len)
        .unwrap_or(0);
    let bot_db = &ctx.data().bot_db;
    bot_db.set_active(ctx.channel_id(), true).await?;
    let http = ctx.serenity_context().http.clone();
    let cm = crate::monitor::ChannelMonitor::init(
        http,
        ctx.channel_id(),
        ctx.data().tgtg_bindings.clone(),
        ctx.data().bot_db.clone(),
        ctx.data().watches.clone(),
        ctx.data().last_fired.clone(),
    );

    ctx.data().active_channels.write().await.insert(cm);
    info!("Channel {}: Monitor starting", ctx.channel_id());
    if watch_count == 0 {
        ctx.reply("Started monitoring! You have no watches yet, add some with /watch.")
            .await?;
    } else {
        ctx.reply(format!("Started monitoring {} watch(s)!", watch_count))
            .await?;
    }
    Ok(())
}

/// Stop monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    // Are we already monitoring
    if ctx
        .data()
        .active_channels
        .read()
        .await
        .iter()
        .all(|c| c.channel_id != ctx.channel_id())
    {
        ctx.reply("There is nothing to stop!").await?;
        return Ok(());
    }

    ctx.data()
        .active_channels
        .write()
        .await
        .retain(|c| c.channel_id != ctx.channel_id());

    let bot_db = &ctx.data().bot_db;
    bot_db.set_active(ctx.channel_id(), false).await?;

    ctx.reply("Stopped monitoring!").await?;
    info!("Channel {}: Monitor stopping", ctx.channel_id());

    Ok(())
}

/// Resolves a browse/lookup origin: explicit coordinates > remembered
/// `/list-shops` area > existing location watch of the channel.
async fn resolve_origin(
    ctx: Context<'_>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    radius_override: Option<u8>,
) -> Result<BrowseOrigin> {
    if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
        return Ok(BrowseOrigin {
            latitude,
            longitude,
            radius: radius_override.unwrap_or(DEFAULT_BROWSE_RADIUS),
        });
    }
    if let Some(mut origin) = ctx
        .data()
        .browse_origins
        .read()
        .await
        .get(&ctx.channel_id())
        .copied()
    {
        if let Some(radius) = radius_override {
            origin.radius = radius;
        }
        return Ok(origin);
    }
    let location = ctx
        .data()
        .watches
        .read()
        .await
        .get(&ctx.channel_id())
        .and_then(|channel_watches| {
            channel_watches.iter().find_map(|w| match &w.details {
                WatchDetails::Location {
                    latitude,
                    longitude,
                    radius,
                    ..
                } => Some(BrowseOrigin {
                    latitude: *latitude,
                    longitude: *longitude,
                    radius: *radius,
                }),
                _ => None,
            })
        });
    if let Some(mut origin) = location {
        if let Some(radius) = radius_override {
            origin.radius = radius;
        }
        return Ok(origin);
    }
    anyhow::bail!("Could not determine an area for this action.");
}
