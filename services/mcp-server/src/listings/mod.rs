pub mod filters;
pub mod models;
pub mod sources;

use chrono::Utc;
use firestore::FirestoreDb;
use models::{
    AgentJob, HealthCheck, IngestionConfig, ListingDoc, ListingEventDoc, RawListing,
    SourceHealthDoc, INGESTION_CONFIG, LISTINGS, LISTING_EVENTS, SOURCE_HEALTH,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Cached listings with a TTL to avoid hammering external APIs on rapid successive calls.
pub struct ListingsCache {
    listings: Vec<AgentJob>,
    fetched_at: chrono::DateTime<Utc>,
}

const CACHE_TTL_SECS: i64 = 300; // 5 minutes

/// State for the listings subsystem, added to SharedState.
pub struct ListingsState {
    pub db: FirestoreDb,
    pub http_client: reqwest::Client,
    cache: Mutex<Option<ListingsCache>>,
}

impl ListingsState {
    pub fn new(db: FirestoreDb, http_client: reqwest::Client) -> Self {
        Self {
            db,
            http_client,
            cache: Mutex::new(None),
        }
    }
}

/// Fetch listings, update Firestore, return filtered results.
/// This is called by the GET /internal/listings endpoint.
pub async fn get_listings(state: &Arc<ListingsState>) -> Result<Vec<AgentJob>, anyhow::Error> {
    // Check cache
    {
        let cache = state.cache.lock().await;
        if let Some(ref cached) = *cache {
            let age = Utc::now()
                .signed_duration_since(cached.fetched_at)
                .num_seconds();
            if age < CACHE_TTL_SECS {
                tracing::debug!(cache_age_secs = age, "serving listings from cache");
                return Ok(cached.listings.clone());
            }
        }
    }

    // Fetch from all sources in parallel
    let client = &state.http_client;
    let (clawtasks, botbounty, bountycaster, moltlaunch, shillbot) = tokio::join!(
        sources::fetch_clawtasks(client),
        sources::fetch_botbounty(client),
        sources::fetch_bountycaster(client),
        sources::fetch_moltlaunch(client),
        sources::fetch_shillbot(client),
    );

    let fetch_results = vec![clawtasks, botbounty, bountycaster, moltlaunch, shillbot];

    // Load ingestion config (fallback to defaults if not in Firestore)
    let config = load_ingestion_config(&state.db).await;

    // Collect all raw listings and deduplicate
    let mut seen = std::collections::HashSet::new();
    let mut all_raw: Vec<RawListing> = Vec::new();
    for result in &fetch_results {
        for listing in &result.listings {
            let key = listing.doc_id();
            if seen.insert(key) {
                all_raw.push(listing.clone());
            }
        }
    }

    tracing::info!(
        total_fetched = all_raw.len(),
        "fetched listings from external sources"
    );

    // Load existing listings from Firestore for diffing
    let existing = load_existing_listings(&state.db).await;

    // Process each listing: filter, upsert, emit events
    let now = Utc::now();
    let mut active_doc_ids = std::collections::HashSet::new();
    let mut result_listings: Vec<ListingDoc> = Vec::new();

    for raw in &all_raw {
        let filter_result = filters::apply_filters(raw, &config);
        let doc_id = raw.doc_id();
        active_doc_ids.insert(doc_id.clone());

        let doc = if let Some(existing_doc) = existing.get(&doc_id) {
            // Existing listing: update last_seen_at, possibly reappear
            let mut updated = existing_doc.clone();
            updated.last_seen_at = now;
            updated.filtered = filter_result.filtered;
            updated.filter_reason = filter_result.reason;
            // Update fields that may have changed at source
            updated.title.clone_from(&raw.title);
            updated.description.clone_from(&raw.description);
            updated.reward_amount.clone_from(&raw.reward_amount);
            updated.reward_usd_estimate = raw.reward_usd_estimate;

            if updated.status == "disappeared" {
                updated.status = "open".to_string();
                updated.disappeared_at = None;
                emit_event(&state.db, &doc_id, "reappeared", None, Some("open")).await;
            }

            updated
        } else {
            // New listing
            let doc = ListingDoc {
                source: raw.source.clone(),
                source_id: raw.source_id.clone(),
                source_url: raw.source_url.clone(),
                title: raw.title.clone(),
                description: raw.description.clone(),
                category: raw.category.clone(),
                tags: raw.tags.clone(),
                reward_amount: raw.reward_amount.clone(),
                reward_token: raw.reward_token.clone(),
                reward_chain: raw.reward_chain.clone(),
                reward_usd_estimate: raw.reward_usd_estimate,
                payment_model: raw.payment_model.clone(),
                escrow: raw.escrow,
                posted_at: raw.posted_at,
                deadline: raw.deadline,
                status: "open".to_string(),
                first_seen_at: now,
                last_seen_at: now,
                disappeared_at: None,
                filtered: filter_result.filtered,
                filter_reason: filter_result.reason,
            };
            emit_event(&state.db, &doc_id, "first_seen", None, None).await;
            doc
        };

        // Upsert to Firestore
        upsert_listing(&state.db, &doc).await;

        if !doc.filtered && doc.status == "open" {
            result_listings.push(doc);
        }
    }

    // Mark disappeared listings
    for (doc_id, existing_doc) in &existing {
        if existing_doc.status == "open" && !active_doc_ids.contains(doc_id) {
            let mut disappeared = existing_doc.clone();
            disappeared.status = "disappeared".to_string();
            disappeared.disappeared_at = Some(now);
            upsert_listing(&state.db, &disappeared).await;
            emit_event(&state.db, doc_id, "disappeared", Some("open"), None).await;
        }
    }

    // Record source health
    for result in &fetch_results {
        record_source_health(&state.db, &result.source, &result.health).await;
    }

    // Sort: most recent first
    result_listings.sort_by(|a, b| b.posted_at.cmp(&a.posted_at));

    let agent_jobs: Vec<AgentJob> = result_listings.iter().map(AgentJob::from).collect();

    // Update cache
    {
        let mut cache = state.cache.lock().await;
        *cache = Some(ListingsCache {
            listings: agent_jobs.clone(),
            fetched_at: now,
        });
    }

    tracing::info!(returned = agent_jobs.len(), "listings ingestion complete");

    Ok(agent_jobs)
}

// -- Firestore helpers --

async fn load_ingestion_config(db: &FirestoreDb) -> IngestionConfig {
    match db
        .fluent()
        .select()
        .by_id_in(INGESTION_CONFIG)
        .obj::<IngestionConfig>()
        .one("default")
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => {
            tracing::info!("no ingestion_config/default in Firestore, using defaults");
            IngestionConfig::default()
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to load ingestion config, using defaults");
            IngestionConfig::default()
        }
    }
}

async fn load_existing_listings(db: &FirestoreDb) -> HashMap<String, ListingDoc> {
    match db
        .fluent()
        .select()
        .from(LISTINGS)
        .obj::<ListingDoc>()
        .query()
        .await
    {
        Ok(docs) => docs.into_iter().map(|d| (d.doc_id(), d)).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load existing listings");
            HashMap::new()
        }
    }
}

async fn upsert_listing(db: &FirestoreDb, doc: &ListingDoc) {
    let doc_id = doc.doc_id();
    if let Err(e) = db
        .fluent()
        .update()
        .in_col(LISTINGS)
        .document_id(&doc_id)
        .object(doc)
        .execute::<ListingDoc>()
        .await
    {
        tracing::warn!(doc_id = %doc_id, error = %e, "failed to upsert listing");
    }
}

async fn emit_event(
    db: &FirestoreDb,
    listing_id: &str,
    event_type: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
) {
    let event = ListingEventDoc {
        listing_id: listing_id.to_string(),
        event_type: event_type.to_string(),
        old_value: old_value.map(String::from),
        new_value: new_value.map(String::from),
        timestamp: Utc::now(),
    };

    let event_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = db
        .fluent()
        .insert()
        .into(LISTING_EVENTS)
        .document_id(&event_id)
        .object(&event)
        .execute::<ListingEventDoc>()
        .await
    {
        tracing::warn!(
            listing_id = %listing_id,
            event_type = %event_type,
            error = %e,
            "failed to emit listing event"
        );
    }
}

async fn record_source_health(db: &FirestoreDb, source: &str, check: &HealthCheck) {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let doc_id = format!("{source}:{date}");

    // Try to load existing doc and append, or create new
    let mut health_doc = match db
        .fluent()
        .select()
        .by_id_in(SOURCE_HEALTH)
        .obj::<SourceHealthDoc>()
        .one(&doc_id)
        .await
    {
        Ok(Some(existing)) => existing,
        _ => SourceHealthDoc {
            source: source.to_string(),
            date: date.clone(),
            checks: Vec::new(),
            total_checks: 0,
            successful_checks: 0,
        },
    };

    health_doc.checks.push(check.clone());
    health_doc.total_checks = health_doc.total_checks.saturating_add(1);
    if check.error.is_none() && check.status_code >= 200 && check.status_code < 300 {
        health_doc.successful_checks = health_doc.successful_checks.saturating_add(1);
    }

    if let Err(e) = db
        .fluent()
        .update()
        .in_col(SOURCE_HEALTH)
        .document_id(&doc_id)
        .object(&health_doc)
        .execute::<SourceHealthDoc>()
        .await
    {
        tracing::warn!(source = %source, error = %e, "failed to record source health");
    }
}

/// Build the axum handler for GET /internal/listings.
pub fn listings_handler(state: Arc<ListingsState>) -> axum::routing::MethodRouter {
    axum::routing::get(move || {
        let state = state.clone();
        async move {
            match get_listings(&state).await {
                Ok(listings) => axum::Json(listings).into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "listings ingestion failed");
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("{{\"error\": \"{e}\"}}"),
                    )
                        .into_response()
                }
            }
        }
    })
}

use axum::response::IntoResponse;
