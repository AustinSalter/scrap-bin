use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_TWITTER};
use crate::config::{self, ConfigError};
use crate::fragment::{self, Fragment, SourceType};
use crate::grpc_client::{get_grpc_client, GrpcError};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl, TokenUrl,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),
    #[error("OAuth error: {0}")]
    OAuth(String),
    #[error("Twitter authorization required")]
    AuthRequired,
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),
    #[error("Rate limited, retry after {0} seconds")]
    RateLimited(u64),
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<reqwest::Error> for SourceError {
    fn from(err: reqwest::Error) -> Self {
        SourceError::Http(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Twitter API v2 response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct BookmarksResponse {
    data: Option<Vec<TweetData>>,
    includes: Option<Includes>,
    meta: Option<BookmarksMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetData {
    pub id: String,
    pub text: String,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
    pub conversation_id: Option<String>,
    pub in_reply_to_user_id: Option<String>,
    pub referenced_tweets: Option<Vec<ReferencedTweet>>,
    pub entities: Option<TweetEntities>,
    pub note_tweet: Option<ApiNoteTweet>,
    pub public_metrics: Option<PublicMetrics>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReferencedTweet {
    #[serde(rename = "type")]
    pub ref_type: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetEntities {
    pub urls: Option<Vec<TweetUrl>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetUrl {
    pub expanded_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublicMetrics {
    pub retweet_count: Option<u64>,
    pub reply_count: Option<u64>,
    pub like_count: Option<u64>,
    pub quote_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiNoteTweet {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserData {
    pub id: String,
    pub username: String,
    pub name: String,
    pub profile_image_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Includes {
    users: Option<Vec<UserData>>,
    tweets: Option<Vec<TweetData>>,
}

#[derive(Debug, Clone, Deserialize)]
struct BookmarksMeta {
    result_count: Option<usize>,
    next_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UsersMeResponse {
    data: Option<UsersMeData>,
}

#[derive(Debug, Clone, Deserialize)]
struct UsersMeData {
    id: String,
    username: String,
}

// ---------------------------------------------------------------------------
// OAuth token exchange response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

// ---------------------------------------------------------------------------
// OAuth 2.0 PKCE flow
// ---------------------------------------------------------------------------

struct PendingAuth {
    pkce_verifier: String,
    csrf_state: String,
    redirect_uri: String,
    client_id: String,
}

static PENDING_AUTH: Mutex<Option<PendingAuth>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStartResult {
    pub auth_url: String,
    pub state: String,
}

/// Starts the OAuth 2.0 PKCE flow for Twitter. Returns the authorization URL
/// that the frontend should open in the system browser.
#[tauri::command]
pub async fn source_twitter_auth_start(
    client_id: String,
) -> Result<AuthStartResult, SourceError> {
    let auth_url_str = "https://twitter.com/i/oauth2/authorize";
    let token_url_str = "https://api.twitter.com/2/oauth2/token";

    // Bind a TCP listener to a random available port for the callback
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| SourceError::OAuth(format!("Failed to bind callback listener: {}", e)))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| SourceError::OAuth(format!("Failed to get local addr: {}", e)))?;
    let redirect_uri = format!("http://127.0.0.1:{}", local_addr.port());

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let csrf_state = CsrfToken::new_random();

    // Build authorization URL
    let oauth_client = BasicClient::new(ClientId::new(client_id.clone()))
        .set_auth_uri(AuthUrl::new(auth_url_str.to_string()).map_err(|e| {
            SourceError::OAuth(format!("Invalid auth URL: {}", e))
        })?)
        .set_token_uri(TokenUrl::new(token_url_str.to_string()).map_err(|e| {
            SourceError::OAuth(format!("Invalid token URL: {}", e))
        })?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.clone()).map_err(|e| {
            SourceError::OAuth(format!("Invalid redirect URL: {}", e))
        })?);

    let (auth_url, state) = oauth_client
        .authorize_url(|| csrf_state)
        .add_scope(oauth2::Scope::new("tweet.read".to_string()))
        .add_scope(oauth2::Scope::new("users.read".to_string()))
        .add_scope(oauth2::Scope::new("bookmark.read".to_string()))
        .add_scope(oauth2::Scope::new("offline.access".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let state_str = state.secret().clone();

    // Store pending auth state
    {
        let mut pending = PENDING_AUTH.lock();
        if pending.is_some() {
            tracing::warn!("Overwriting existing pending Twitter auth state — previous auth flow will be abandoned");
        }
        *pending = Some(PendingAuth {
            pkce_verifier: pkce_verifier.secret().clone(),
            csrf_state: state_str.clone(),
            redirect_uri: redirect_uri.clone(),
            client_id: client_id.clone(),
        });
    }

    // Spawn background task to listen for callback
    tokio::spawn(async move {
        if let Err(e) = listen_for_callback(listener).await {
            tracing::error!("OAuth callback listener error: {}", e);
        }
    });

    Ok(AuthStartResult {
        auth_url: auth_url.to_string(),
        state: state_str,
    })
}

/// Listens for the OAuth callback on the TCP listener.
async fn listen_for_callback(listener: tokio::net::TcpListener) -> Result<(), SourceError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Wait up to 5 minutes for the callback
    let accept_result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        listener.accept(),
    )
    .await
    .map_err(|_| SourceError::OAuth("OAuth callback timed out after 5 minutes".to_string()))?
    .map_err(|e| SourceError::OAuth(format!("Failed to accept connection: {}", e)))?;

    let (mut stream, _addr) = accept_result;

    // Read the HTTP request
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| SourceError::OAuth(format!("Failed to read request: {}", e)))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse GET request line to extract query params
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");

    // Parse query string
    let query_string = path.split('?').nth(1).unwrap_or("");
    let params: HashMap<String, String> =
        url::form_urlencoded::parse(query_string.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

    let code = params.get("code").cloned();
    let state = params.get("state").cloned();

    // Retrieve and clear pending auth
    let pending = {
        let mut guard = PENDING_AUTH.lock();
        guard.take()
    };

    let pending = pending.ok_or_else(|| {
        SourceError::OAuth("No pending auth state found".to_string())
    })?;

    // Validate state
    if state.as_deref() != Some(&pending.csrf_state) {
        let response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authorization Failed</h1><p>CSRF state mismatch. Please try again.</p></body></html>";
        let _ = stream.write_all(response.as_bytes()).await;
        return Err(SourceError::OAuth("CSRF state mismatch".to_string()));
    }

    let code = code.ok_or_else(|| {
        SourceError::OAuth("No authorization code in callback".to_string())
    })?;

    // Exchange code for tokens
    match exchange_code_for_tokens(
        &pending.client_id,
        &code,
        &pending.redirect_uri,
        &pending.pkce_verifier,
    )
    .await
    {
        Ok(_creds) => {
            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authorization Complete</h1><p>You can close this tab and return to Scrapbin.</p></body></html>";
            let _ = stream.write_all(response.as_bytes()).await;
            tracing::info!("Twitter OAuth flow completed successfully");
            Ok(())
        }
        Err(e) => {
            tracing::error!("OAuth token exchange failed: {}", e);
            let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authorization Failed</h1><p>Something went wrong during authorization. Please try again from Scrapbin.</p></body></html>";
            let _ = stream.write_all(response.as_bytes()).await;
            Err(e)
        }
    }
}

/// Exchanges an authorization code for tokens, fetches user info, and saves credentials.
async fn exchange_code_for_tokens(
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
) -> Result<config::TwitterCredentials, SourceError> {
    let http_client = reqwest::Client::new();

    let resp = http_client
        .post("https://api.twitter.com/2/oauth2/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", pkce_verifier),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(SourceError::OAuth(format!(
            "Token exchange failed ({}): {}",
            status, body
        )));
    }

    let token_resp: TokenResponse = resp.json().await?;

    let refresh_token = token_resp
        .refresh_token
        .ok_or_else(|| SourceError::OAuth("No refresh token in response".to_string()))?;

    // Fetch user info
    let user_resp = http_client
        .get("https://api.twitter.com/2/users/me")
        .bearer_auth(&token_resp.access_token)
        .send()
        .await?;

    if !user_resp.status().is_success() {
        let status = user_resp.status();
        let body = user_resp.text().await.unwrap_or_default();
        return Err(SourceError::OAuth(format!(
            "Failed to fetch user info ({}): {}",
            status, body
        )));
    }

    let me: UsersMeResponse = user_resp.json().await?;
    let me_data = me
        .data
        .ok_or_else(|| SourceError::OAuth("No user data in /users/me response".to_string()))?;

    let expires_at = if let Some(expires_in) = token_resp.expires_in {
        (chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64)).to_rfc3339()
    } else {
        // Default to 2 hours if not specified
        (chrono::Utc::now() + chrono::Duration::seconds(7200)).to_rfc3339()
    };

    let creds = config::TwitterCredentials {
        access_token: token_resp.access_token,
        refresh_token,
        user_id: me_data.id,
        username: me_data.username,
        expires_at,
    };

    config::save_twitter_credentials(&creds)?;
    tracing::info!("Twitter credentials saved for user @{}", creds.username);

    Ok(creds)
}

/// Refreshes the access token using the stored refresh token.
async fn refresh_access_token(client_id: &str) -> Result<config::TwitterCredentials, SourceError> {
    let creds = config::load_twitter_credentials()
        .ok_or(SourceError::AuthRequired)?;

    let http_client = reqwest::Client::new();

    let resp = http_client
        .post("https://api.twitter.com/2/oauth2/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &creds.refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await
        .map_err(|e| SourceError::TokenRefreshFailed(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(SourceError::TokenRefreshFailed(format!(
            "Refresh failed ({}): {}",
            status, body
        )));
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| SourceError::TokenRefreshFailed(e.to_string()))?;

    let refresh_token = token_resp
        .refresh_token
        .unwrap_or(creds.refresh_token);

    let expires_at = if let Some(expires_in) = token_resp.expires_in {
        (chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64)).to_rfc3339()
    } else {
        (chrono::Utc::now() + chrono::Duration::seconds(7200)).to_rfc3339()
    };

    let new_creds = config::TwitterCredentials {
        access_token: token_resp.access_token,
        refresh_token,
        user_id: creds.user_id,
        username: creds.username,
        expires_at,
    };

    config::save_twitter_credentials(&new_creds)?;
    tracing::info!("Twitter access token refreshed");

    Ok(new_creds)
}

/// Returns a valid access token, refreshing if within 5 minutes of expiry.
async fn get_valid_token(client_id: &str) -> Result<String, SourceError> {
    let creds = config::load_twitter_credentials()
        .ok_or(SourceError::AuthRequired)?;

    // Check if token is about to expire (within 5 minutes)
    if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&creds.expires_at) {
        let now = chrono::Utc::now();
        let buffer = chrono::Duration::seconds(300);
        if now + buffer >= expires_at {
            tracing::info!("Twitter token near expiry, refreshing...");
            let new_creds = refresh_access_token(client_id).await?;
            return Ok(new_creds.access_token);
        }
    }

    Ok(creds.access_token)
}

// ---------------------------------------------------------------------------
// Sync state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TwitterSyncState {
    last_sync_at: Option<String>,
    pagination_cursor: Option<String>,
}

fn load_sync_state() -> TwitterSyncState {
    config::twitter_sync_path()
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_sync_state(state: &TwitterSyncState) -> Result<(), SourceError> {
    let path = config::twitter_sync_path()?;
    let data = serde_json::to_string_pretty(state)?;
    std::fs::write(path, data)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Bookmark fetching with pagination
// ---------------------------------------------------------------------------

struct FetchResult {
    tweets: Vec<TweetData>,
    users: Vec<UserData>,
    ref_tweets: Vec<TweetData>,
}

async fn fetch_bookmarks(
    access_token: &str,
    user_id: &str,
    existing_tweet_ids: &HashSet<String>,
) -> Result<FetchResult, SourceError> {
    let http_client = reqwest::Client::new();

    let tweet_fields = "created_at,author_id,conversation_id,in_reply_to_user_id,referenced_tweets,entities,note_tweet,public_metrics";
    let user_fields = "username,name,profile_image_url";
    let expansions = "author_id,referenced_tweets.id,referenced_tweets.id.author_id";

    let mut all_tweets = Vec::new();
    let mut all_users: Vec<UserData> = Vec::new();
    let mut all_ref_tweets: Vec<TweetData> = Vec::new();

    // Resume from saved cursor if available
    let sync_state = load_sync_state();
    let mut next_token = sync_state.pagination_cursor.clone();
    let mut page_count = 0u32;
    const MAX_PAGES: u32 = 50; // Safety cap: 50 pages × 100 = 5000 bookmarks max

    loop {
        let mut url = format!(
            "https://api.twitter.com/2/users/{}/bookmarks?max_results=100&tweet.fields={}&user.fields={}&expansions={}",
            user_id, tweet_fields, user_fields, expansions
        );

        if let Some(ref token) = next_token {
            url.push_str(&format!("&pagination_token={}", token));
        }

        tracing::debug!("Fetching bookmarks page {} (cursor: {:?})", page_count, next_token);

        let resp = http_client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await?;

        // Handle rate limiting
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(900); // default 15 min

            // Save cursor so we can resume later
            let mut state = load_sync_state();
            state.pagination_cursor = next_token;
            let _ = save_sync_state(&state);

            return Err(SourceError::RateLimited(retry_after));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SourceError::Http(format!(
                "Bookmarks API returned {}: {}",
                status, body
            )));
        }

        let page: BookmarksResponse = resp.json().await?;

        // Collect users from includes
        if let Some(ref includes) = page.includes {
            if let Some(ref users) = includes.users {
                all_users.extend(users.iter().cloned());
            }
            if let Some(ref tweets) = includes.tweets {
                all_ref_tweets.extend(tweets.iter().cloned());
            }
        }

        // Collect tweets
        let page_tweets = page.data.unwrap_or_default();

        // Check for early exit: if all tweets on this page already exist, stop
        if !page_tweets.is_empty() {
            let all_existing = page_tweets
                .iter()
                .all(|t| existing_tweet_ids.contains(&t.id));
            if all_existing {
                tracing::info!("All bookmarks on page already indexed, stopping pagination");
                break;
            }
        }

        all_tweets.extend(page_tweets);

        // Check for next page
        next_token = page.meta.and_then(|m| m.next_token);
        if next_token.is_none() {
            break;
        }

        page_count += 1;
        if page_count >= MAX_PAGES {
            tracing::warn!("Reached pagination safety cap ({} pages), stopping", MAX_PAGES);
            break;
        }
    }

    Ok(FetchResult {
        tweets: all_tweets,
        users: all_users,
        ref_tweets: all_ref_tweets,
    })
}

// ---------------------------------------------------------------------------
// Rich metadata extraction + thread stitching
// ---------------------------------------------------------------------------

/// Resolves an author_id to (handle, name, avatar_url) from the users expansion list.
pub fn resolve_author(
    author_id: &str,
    users: &[UserData],
) -> (Option<String>, Option<String>, Option<String>) {
    users
        .iter()
        .find(|u| u.id == author_id)
        .map(|u| {
            (
                Some(u.username.clone()),
                Some(u.name.clone()),
                u.profile_image_url.clone(),
            )
        })
        .unwrap_or((None, None, None))
}

/// Groups tweets by conversation_id and assigns thread positions.
/// Returns Vec of (tweet, thread_position). Single-tweet groups get None.
pub fn stitch_threads(tweets: &[TweetData]) -> Vec<(&TweetData, Option<usize>)> {
    // Group by conversation_id (or own id if no conversation_id)
    let mut groups: HashMap<String, Vec<&TweetData>> = HashMap::new();
    for tweet in tweets {
        let conv_id = tweet
            .conversation_id
            .as_deref()
            .unwrap_or(&tweet.id)
            .to_string();
        groups.entry(conv_id).or_default().push(tweet);
    }

    let mut result = Vec::new();

    for (_conv_id, mut group) in groups {
        // Sort within group by created_at ascending
        group.sort_by(|a, b| {
            let a_time = a.created_at.as_deref().unwrap_or("");
            let b_time = b.created_at.as_deref().unwrap_or("");
            a_time.cmp(b_time)
        });

        if group.len() == 1 {
            result.push((group[0], None));
        } else {
            for (pos, tweet) in group.into_iter().enumerate() {
                result.push((tweet, Some(pos)));
            }
        }
    }

    result
}

/// Converts a single API tweet into one or more Fragments with rich metadata.
pub fn api_tweet_to_fragments(
    tweet: &TweetData,
    users: &[UserData],
    ref_tweets: &[TweetData],
    thread_position: Option<usize>,
) -> Vec<Fragment> {
    // Prefer note_tweet.text over truncated text
    let full_text = tweet
        .note_tweet
        .as_ref()
        .and_then(|nt| nt.text.as_deref())
        .unwrap_or(&tweet.text);

    let source_path_str = format!("twitter://bookmark/{}", tweet.id);
    let chunked = crate::chunker::chunk_plain_text(full_text, &source_path_str);
    let chunks: Vec<String> = chunked.iter().map(|c| c.content.clone()).collect();
    let modified_at = tweet
        .created_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    // Resolve author
    let (author_handle, author_name, author_avatar_url) = tweet
        .author_id
        .as_deref()
        .map(|aid| resolve_author(aid, users))
        .unwrap_or((None, None, None));

    // Determine reply/quote status
    let is_reply = tweet.in_reply_to_user_id.is_some();
    let is_quote = tweet
        .referenced_tweets
        .as_ref()
        .map(|refs| refs.iter().any(|r| r.ref_type == "quoted"))
        .unwrap_or(false);

    // Find quoted tweet text
    let quoted_tweet_text = if is_quote {
        tweet
            .referenced_tweets
            .as_ref()
            .and_then(|refs| {
                refs.iter()
                    .find(|r| r.ref_type == "quoted")
                    .and_then(|r| {
                        ref_tweets
                            .iter()
                            .find(|t| t.id == r.id)
                            .map(|t| {
                                t.note_tweet
                                    .as_ref()
                                    .and_then(|nt| nt.text.clone())
                                    .unwrap_or_else(|| t.text.clone())
                            })
                    })
            })
    } else {
        None
    };

    // Collect URLs from entities
    let urls_str = tweet
        .entities
        .as_ref()
        .and_then(|e| e.urls.as_ref())
        .map(|urls| {
            urls.iter()
                .filter_map(|u| u.expanded_url.as_deref().or(u.url.as_deref()))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();

    // Build tweet URL
    let tweet_url = author_handle
        .as_deref()
        .map(|handle| format!("https://twitter.com/{}/status/{}", handle, tweet.id))
        .unwrap_or_else(|| format!("https://twitter.com/i/status/{}", tweet.id));

    // Thread info
    let conversation_id = tweet.conversation_id.as_deref().unwrap_or(&tweet.id);

    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk_text)| {
            let hash = fragment::content_hash(&chunk_text);
            let token_count = fragment::estimate_tokens(&chunk_text);
            let id = ulid::Ulid::new().to_string();

            let mut metadata = serde_json::json!({
                "tweet_id": tweet.id,
                "tweet_url": tweet_url,
                "conversation_id": conversation_id,
                "thread_id": conversation_id,
                "is_reply": is_reply,
                "is_quote": is_quote,
            });

            if let Some(ref aid) = tweet.author_id {
                metadata["author_id"] = serde_json::json!(aid);
            }
            if let Some(ref handle) = author_handle {
                metadata["author_handle"] = serde_json::json!(handle);
            }
            if let Some(ref name) = author_name {
                metadata["author_name"] = serde_json::json!(name);
            }
            if let Some(ref avatar) = author_avatar_url {
                metadata["author_avatar_url"] = serde_json::json!(avatar);
            }
            if let Some(pos) = thread_position {
                metadata["thread_position"] = serde_json::json!(pos);
            }
            if let Some(ref qt) = quoted_tweet_text {
                metadata["quoted_tweet_text"] = serde_json::json!(qt);
            }
            if !urls_str.is_empty() {
                metadata["urls"] = serde_json::json!(&urls_str);
            }
            if let Some(ref metrics) = tweet.public_metrics {
                if let Some(count) = metrics.like_count {
                    metadata["like_count"] = serde_json::json!(count);
                }
                if let Some(count) = metrics.retweet_count {
                    metadata["retweet_count"] = serde_json::json!(count);
                }
                if let Some(count) = metrics.reply_count {
                    metadata["reply_count"] = serde_json::json!(count);
                }
            }

            Fragment {
                id,
                content: chunk_text,
                source_type: SourceType::Twitter,
                source_path: source_path_str.clone(),
                chunk_index: idx,
                heading_path: Vec::new(),
                tags: Vec::new(),
                token_count,
                content_hash: hash,
                modified_at: modified_at.clone(),
                cluster_id: None,
                disposition: fragment::Disposition::Signal,
                metadata,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tauri commands — API sync
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterSyncResult {
    pub imported: usize,
    pub skipped: usize,
    pub threads_detected: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterConnectionInfo {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub connected: bool,
}

/// Syncs Twitter bookmarks via the API: fetches, deduplicates, stitches threads,
/// embeds, and stores in Chroma.
#[tauri::command]
pub async fn source_twitter_sync(
    client_id: String,
) -> Result<TwitterSyncResult, SourceError> {
    // Get valid token (auto-refresh if needed)
    let access_token = get_valid_token(&client_id).await?;
    let creds = config::load_twitter_credentials()
        .ok_or(SourceError::AuthRequired)?;

    // Query Chroma for existing tweet IDs
    let chroma_client = get_client();
    let coll_id = get_collection_id(COLLECTION_TWITTER).await?;
    let mut existing_ids = HashSet::new();
    let existing_result = chroma_client
        .get(&coll_id, None, None, Some(vec!["metadatas".to_string()]), None, None)
        .await;
    if let Ok(result) = existing_result {
        if let Some(metas) = &result.metadatas {
            for meta in metas.iter().flatten() {
                if let Some(tid) = meta.get("tweet_id").and_then(|v| v.as_str()) {
                    existing_ids.insert(tid.to_string());
                }
            }
        }
    }

    // Fetch bookmarks from API
    let fetch_result = fetch_bookmarks(&access_token, &creds.user_id, &existing_ids).await?;

    // Filter to new tweets only
    let new_tweets: Vec<&TweetData> = fetch_result
        .tweets
        .iter()
        .filter(|t| !existing_ids.contains(&t.id))
        .collect();

    let skipped = fetch_result.tweets.len() - new_tweets.len();

    if new_tweets.is_empty() {
        // Save sync state even when nothing new
        let mut state = load_sync_state();
        state.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        state.pagination_cursor = None;
        let _ = save_sync_state(&state);

        return Ok(TwitterSyncResult {
            imported: 0,
            skipped,
            threads_detected: 0,
            errors: Vec::new(),
        });
    }

    // Stitch threads
    let owned_new_tweets: Vec<TweetData> = new_tweets
        .iter()
        .map(|t| (*t).clone())
        .collect();
    let threaded = stitch_threads(&owned_new_tweets);
    let threads_detected = {
        let mut conv_ids: HashSet<&str> = HashSet::new();
        for (tweet, pos) in &threaded {
            if pos.is_some() {
                let cid = tweet.conversation_id.as_deref().unwrap_or(&tweet.id);
                conv_ids.insert(cid);
            }
        }
        conv_ids.len()
    };

    // Convert to fragments
    let mut all_fragments = Vec::new();
    let mut errors = Vec::new();
    let mut imported = 0usize;

    for (tweet, thread_pos) in &threaded {
        let tweet_text = tweet
            .note_tweet
            .as_ref()
            .and_then(|nt| nt.text.as_deref())
            .unwrap_or(&tweet.text);
        if tweet_text.trim().is_empty() {
            errors.push(format!("Tweet {} has empty text, skipped", tweet.id));
            continue;
        }

        let fragments = api_tweet_to_fragments(
            tweet,
            &fetch_result.users,
            &fetch_result.ref_tweets,
            *thread_pos,
        );
        imported += 1;
        all_fragments.extend(fragments);
    }

    // Embed and store
    if !all_fragments.is_empty() {
        let grpc = get_grpc_client()?;
        let texts: Vec<String> = all_fragments.iter().map(|f| f.content.clone()).collect();
        let embeddings = grpc.embed_batch(texts).await?;

        let ids: Vec<String> = all_fragments.iter().map(|f| f.id.clone()).collect();
        let documents: Vec<String> = all_fragments.iter().map(|f| f.content.clone()).collect();
        let metadatas: Vec<serde_json::Value> = all_fragments
            .iter()
            .map(fragment::fragment_to_chroma_metadata)
            .collect();

        chroma_client
            .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
            .await?;

        tracing::info!("Stored {} Twitter API fragments in Chroma", all_fragments.len());
    }

    // Save sync state
    let mut state = load_sync_state();
    state.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
    state.pagination_cursor = None;
    let _ = save_sync_state(&state);

    Ok(TwitterSyncResult {
        imported,
        skipped,
        threads_detected,
        errors,
    })
}

/// Checks whether Twitter credentials are stored and valid by calling /2/users/me.
/// If `client_id` is provided and the token is expired, attempts a refresh first.
#[tauri::command]
pub async fn source_twitter_check_connection(
    client_id: Option<String>,
) -> Result<TwitterConnectionInfo, SourceError> {
    let creds = match config::load_twitter_credentials() {
        Some(c) => c,
        None => {
            return Ok(TwitterConnectionInfo {
                user_id: None,
                username: None,
                connected: false,
            });
        }
    };

    // Try to get a valid (possibly refreshed) token if client_id is available
    let token = if let Some(ref cid) = client_id {
        match get_valid_token(cid).await {
            Ok(t) => t,
            Err(_) => creds.access_token.clone(),
        }
    } else {
        creds.access_token.clone()
    };

    let http_client = reqwest::Client::new();
    let resp = http_client
        .get("https://api.twitter.com/2/users/me")
        .bearer_auth(&token)
        .send()
        .await;

    // Reload creds in case refresh updated them
    let current_creds = config::load_twitter_credentials().unwrap_or(creds);

    match resp {
        Ok(r) if r.status().is_success() => Ok(TwitterConnectionInfo {
            user_id: Some(current_creds.user_id),
            username: Some(current_creds.username),
            connected: true,
        }),
        _ => Ok(TwitterConnectionInfo {
            user_id: Some(current_creds.user_id),
            username: Some(current_creds.username),
            connected: false,
        }),
    }
}

// ===========================================================================
// Legacy JSON import (fallback)
// ===========================================================================

/// A single bookmark entry from the Twitter JSON export.
#[derive(Debug, Clone, Deserialize)]
struct TwitterBookmark {
    id: String,
    text: String,
    /// Long-form tweet text (> 280 chars).
    note_tweet: Option<NoteTweet>,
    created_at: Option<String>,
    author_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NoteTweet {
    text: Option<String>,
}

/// Wrapper for the top-level `{ "data": [...] }` export format.
#[derive(Debug, Clone, Deserialize)]
struct TwitterExport {
    data: Vec<TwitterBookmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterImportResult {
    /// Number of bookmarks successfully converted to fragments.
    pub imported: usize,
    /// Number of bookmarks skipped because they were already ingested.
    pub skipped: usize,
    /// Per-bookmark error messages (non-fatal).
    pub errors: Vec<String>,
}

use crate::chunker;

/// Converts a single Twitter bookmark into one or more `Fragment`s.
fn bookmark_to_fragments(bookmark: &TwitterBookmark) -> Vec<Fragment> {
    // Prefer long-form note_tweet text over the truncated `text` field.
    let full_text = bookmark
        .note_tweet
        .as_ref()
        .and_then(|nt| nt.text.as_deref())
        .unwrap_or(&bookmark.text);

    let source_path_str = format!("twitter://bookmark/{}", bookmark.id);
    let chunked = chunker::chunk_plain_text(full_text, &source_path_str);
    let chunks: Vec<String> = chunked.iter().map(|c| c.content.clone()).collect();
    let modified_at = bookmark
        .created_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk_text)| {
            let hash = fragment::content_hash(&chunk_text);
            let token_count = fragment::estimate_tokens(&chunk_text);
            let id = ulid::Ulid::new().to_string();

            let mut metadata = serde_json::json!({
                "tweet_id": bookmark.id,
            });
            if let Some(ref author) = bookmark.author_id {
                metadata["author_id"] = serde_json::json!(author);
            }

            Fragment {
                id,
                content: chunk_text,
                source_type: SourceType::Twitter,
                source_path: source_path_str.clone(),
                chunk_index: idx,
                heading_path: Vec::new(),
                tags: Vec::new(),
                token_count,
                content_hash: hash,
                modified_at: modified_at.clone(),
                cluster_id: None,
                disposition: fragment::Disposition::Signal,
                metadata,
            }
        })
        .collect()
}

/// Reads a Twitter bookmark JSON export from `path`, parses the `data` array,
/// and converts each bookmark into `Fragment`s.
///
/// Bookmarks whose `tweet_id` appears in `existing_tweet_ids` are skipped for
/// deduplication. The caller is responsible for querying Chroma to populate
/// that set.
fn import_bookmarks(
    path: &str,
    existing_tweet_ids: &HashSet<String>,
) -> Result<(Vec<Fragment>, TwitterImportResult), SourceError> {
    let data = std::fs::read_to_string(path)?;

    // The export may be either `{ "data": [...] }` or a bare array `[...]`.
    let bookmarks: Vec<TwitterBookmark> = if let Ok(export) =
        serde_json::from_str::<TwitterExport>(&data)
    {
        export.data
    } else {
        serde_json::from_str::<Vec<TwitterBookmark>>(&data)?
    };

    tracing::info!("Parsed {} bookmarks from {}", bookmarks.len(), path);

    let mut all_fragments = Vec::new();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for bookmark in &bookmarks {
        // Dedup by tweet_id.
        if existing_tweet_ids.contains(&bookmark.id) {
            skipped += 1;
            continue;
        }

        // Validate minimal data.
        if bookmark.text.trim().is_empty()
            && bookmark
                .note_tweet
                .as_ref()
                .and_then(|nt| nt.text.as_deref())
                .map_or(true, |t| t.trim().is_empty())
        {
            errors.push(format!("Bookmark {} has empty text, skipped", bookmark.id));
            continue;
        }

        let fragments = bookmark_to_fragments(bookmark);
        imported += 1; // count per bookmark, not per chunk
        all_fragments.extend(fragments);
    }

    let result = TwitterImportResult {
        imported,
        skipped,
        errors,
    };

    tracing::info!(
        "Twitter import: {} imported, {} skipped, {} errors",
        result.imported,
        result.skipped,
        result.errors.len()
    );

    Ok((all_fragments, result))
}

/// Reads a Twitter bookmark JSON export, parses bookmarks, chunks long tweets,
/// and returns the resulting fragments alongside import statistics.
///
/// Actual embedding and Chroma storage happen downstream in the pipeline.
#[tauri::command]
pub async fn source_twitter_import(path: String) -> Result<TwitterImportResult, SourceError> {
    // Query Chroma for existing tweet IDs to deduplicate.
    let client = get_client();
    let coll_id = get_collection_id(COLLECTION_TWITTER).await?;
    let existing_result = client.get(&coll_id, None, None, Some(vec!["metadatas".to_string()]), None, None).await;
    let mut existing_ids = HashSet::new();
    if let Ok(result) = existing_result {
        if let Some(metas) = &result.metadatas {
            for meta in metas.iter().flatten() {
                if let Some(tid) = meta.get("tweet_id").and_then(|v| v.as_str()) {
                    existing_ids.insert(tid.to_string());
                }
            }
        }
    }

    let (fragments, result) = tokio::task::spawn_blocking(move || {
        import_bookmarks(&path, &existing_ids)
    })
    .await
    .map_err(|e| SourceError::InvalidData(format!("Task join error: {e}")))?
    ?;

    // Embed and store fragments in Chroma.
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;
        let texts: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let embeddings = grpc.embed_batch(texts).await?;

        let ids: Vec<String> = fragments.iter().map(|f| f.id.clone()).collect();
        let documents: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let metadatas: Vec<serde_json::Value> = fragments
            .iter()
            .map(fragment::fragment_to_chroma_metadata)
            .collect();

        client
            .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
            .await?;

        tracing::info!("Stored {} Twitter fragments in Chroma", fragments.len());
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_to_fragments_short_tweet() {
        let bookmark = TwitterBookmark {
            id: "123456".to_string(),
            text: "This is a short tweet.".to_string(),
            note_tweet: None,
            created_at: Some("2025-01-15T10:00:00Z".to_string()),
            author_id: Some("user_42".to_string()),
        };

        let fragments = bookmark_to_fragments(&bookmark);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].content, "This is a short tweet.");
        assert_eq!(fragments[0].source_type, SourceType::Twitter);
        assert_eq!(fragments[0].chunk_index, 0);
        assert_eq!(fragments[0].metadata["tweet_id"], "123456");
        assert_eq!(fragments[0].metadata["author_id"], "user_42");
    }

    #[test]
    fn test_bookmark_to_fragments_note_tweet() {
        // Use sentences so the chunker can split at sentence boundaries.
        // Each sentence is ~28 chars ≈ 7 tokens; 100 sentences ≈ 700 tokens > MAX_CHUNK_TOKENS(512).
        let long_text = (0..100)
            .map(|i| format!("This is sentence number {}.", i))
            .collect::<Vec<_>>()
            .join(" ");
        let bookmark = TwitterBookmark {
            id: "789".to_string(),
            text: "Truncated version...".to_string(),
            note_tweet: Some(NoteTweet {
                text: Some(long_text),
            }),
            created_at: None,
            author_id: None,
        };

        let fragments = bookmark_to_fragments(&bookmark);
        assert!(fragments.len() >= 2);
        // All fragments share the same source_path.
        let path = &fragments[0].source_path;
        for f in &fragments {
            assert_eq!(&f.source_path, path);
        }
    }

    #[test]
    fn test_import_dedup() {
        let json = serde_json::json!({
            "data": [
                { "id": "1", "text": "First tweet" },
                { "id": "2", "text": "Second tweet" },
            ]
        });
        let tmp = std::env::temp_dir().join("twitter_test_dedup.json");
        std::fs::write(&tmp, serde_json::to_string(&json).unwrap()).unwrap();

        let mut existing = HashSet::new();
        existing.insert("1".to_string());

        let (fragments, result) =
            import_bookmarks(tmp.to_str().unwrap(), &existing).unwrap();

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].metadata["tweet_id"], "2");

        let _ = std::fs::remove_file(&tmp);
    }

    // --- API tweet tests ---

    fn sample_tweet() -> TweetData {
        TweetData {
            id: "111".to_string(),
            text: "This is a test tweet from the API.".to_string(),
            author_id: Some("author_1".to_string()),
            created_at: Some("2025-06-15T12:00:00Z".to_string()),
            conversation_id: Some("111".to_string()),
            in_reply_to_user_id: None,
            referenced_tweets: None,
            entities: None,
            note_tweet: None,
            public_metrics: None,
        }
    }

    fn sample_users() -> Vec<UserData> {
        vec![UserData {
            id: "author_1".to_string(),
            username: "testuser".to_string(),
            name: "Test User".to_string(),
            profile_image_url: Some("https://pbs.twimg.com/photo.jpg".to_string()),
        }]
    }

    #[test]
    fn test_api_tweet_to_fragments_basic() {
        let tweet = sample_tweet();
        let users = sample_users();
        let fragments = api_tweet_to_fragments(&tweet, &users, &[], None);

        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].disposition, fragment::Disposition::Signal);
        assert_eq!(fragments[0].metadata["tweet_id"], "111");
        assert_eq!(fragments[0].metadata["author_handle"], "testuser");
        assert_eq!(
            fragments[0].metadata["tweet_url"],
            "https://twitter.com/testuser/status/111"
        );
        assert_eq!(fragments[0].source_type, SourceType::Twitter);
    }

    #[test]
    fn test_api_tweet_to_fragments_with_quote() {
        let mut tweet = sample_tweet();
        tweet.referenced_tweets = Some(vec![ReferencedTweet {
            ref_type: "quoted".to_string(),
            id: "222".to_string(),
        }]);

        let ref_tweets = vec![TweetData {
            id: "222".to_string(),
            text: "The original quoted tweet.".to_string(),
            author_id: None,
            created_at: None,
            conversation_id: None,
            in_reply_to_user_id: None,
            referenced_tweets: None,
            entities: None,
            note_tweet: None,
            public_metrics: None,
        }];

        let users = sample_users();
        let fragments = api_tweet_to_fragments(&tweet, &users, &ref_tweets, None);

        assert_eq!(fragments[0].metadata["is_quote"], true);
        assert_eq!(
            fragments[0].metadata["quoted_tweet_text"],
            "The original quoted tweet."
        );
    }

    #[test]
    fn test_api_tweet_to_fragments_with_reply() {
        let mut tweet = sample_tweet();
        tweet.in_reply_to_user_id = Some("other_user".to_string());

        let fragments = api_tweet_to_fragments(&tweet, &sample_users(), &[], None);
        assert_eq!(fragments[0].metadata["is_reply"], true);
    }

    #[test]
    fn test_api_tweet_to_fragments_with_metrics() {
        let mut tweet = sample_tweet();
        tweet.public_metrics = Some(PublicMetrics {
            like_count: Some(42),
            retweet_count: Some(10),
            reply_count: Some(5),
            quote_count: Some(3),
        });

        let fragments = api_tweet_to_fragments(&tweet, &sample_users(), &[], None);
        assert_eq!(fragments[0].metadata["like_count"], 42);
        assert_eq!(fragments[0].metadata["retweet_count"], 10);
        assert_eq!(fragments[0].metadata["reply_count"], 5);
    }

    #[test]
    fn test_api_tweet_to_fragments_urls() {
        let mut tweet = sample_tweet();
        tweet.entities = Some(TweetEntities {
            urls: Some(vec![
                TweetUrl {
                    expanded_url: Some("https://example.com/one".to_string()),
                    url: Some("https://t.co/abc".to_string()),
                },
                TweetUrl {
                    expanded_url: Some("https://example.com/two".to_string()),
                    url: None,
                },
            ]),
        });

        let fragments = api_tweet_to_fragments(&tweet, &sample_users(), &[], None);
        assert_eq!(
            fragments[0].metadata["urls"],
            "https://example.com/one,https://example.com/two"
        );
    }

    #[test]
    fn test_stitch_threads_single() {
        let tweet = sample_tweet();
        let tweets = vec![tweet];
        let result = stitch_threads(&tweets);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, None); // no thread position for singles
    }

    #[test]
    fn test_stitch_threads_multi() {
        let t1 = TweetData {
            id: "1".to_string(),
            text: "First".to_string(),
            author_id: None,
            created_at: Some("2025-01-01T00:00:00Z".to_string()),
            conversation_id: Some("conv_1".to_string()),
            in_reply_to_user_id: None,
            referenced_tweets: None,
            entities: None,
            note_tweet: None,
            public_metrics: None,
        };
        let t2 = TweetData {
            id: "2".to_string(),
            text: "Second".to_string(),
            created_at: Some("2025-01-01T00:01:00Z".to_string()),
            conversation_id: Some("conv_1".to_string()),
            ..t1.clone()
        };
        let t3 = TweetData {
            id: "3".to_string(),
            text: "Third".to_string(),
            created_at: Some("2025-01-01T00:02:00Z".to_string()),
            conversation_id: Some("conv_1".to_string()),
            ..t1.clone()
        };

        let tweets = vec![t3, t1, t2]; // intentionally out of order
        let result = stitch_threads(&tweets);

        assert_eq!(result.len(), 3);
        // All should have thread positions
        let mut positions: Vec<(String, Option<usize>)> = result
            .iter()
            .map(|(t, pos)| (t.id.clone(), *pos))
            .collect();
        positions.sort_by_key(|(_, pos)| *pos);

        assert_eq!(positions[0].1, Some(0));
        assert_eq!(positions[1].1, Some(1));
        assert_eq!(positions[2].1, Some(2));
    }

    #[test]
    fn test_stitch_threads_mixed() {
        let single = TweetData {
            id: "solo".to_string(),
            text: "Solo tweet".to_string(),
            author_id: None,
            created_at: Some("2025-01-01T00:00:00Z".to_string()),
            conversation_id: Some("solo".to_string()),
            in_reply_to_user_id: None,
            referenced_tweets: None,
            entities: None,
            note_tweet: None,
            public_metrics: None,
        };
        let t1 = TweetData {
            id: "a".to_string(),
            text: "Thread A1".to_string(),
            created_at: Some("2025-01-01T00:00:00Z".to_string()),
            conversation_id: Some("thread_a".to_string()),
            ..single.clone()
        };
        let t2 = TweetData {
            id: "b".to_string(),
            text: "Thread A2".to_string(),
            created_at: Some("2025-01-01T00:01:00Z".to_string()),
            conversation_id: Some("thread_a".to_string()),
            ..single.clone()
        };

        let tweets = vec![single, t1, t2];
        let result = stitch_threads(&tweets);

        assert_eq!(result.len(), 3);

        // Find the solo tweet — should have None position
        let solo = result.iter().find(|(t, _)| t.id == "solo").unwrap();
        assert_eq!(solo.1, None);

        // Thread tweets should have Some positions
        let thread_a: Vec<_> = result
            .iter()
            .filter(|(t, _)| t.conversation_id.as_deref() == Some("thread_a"))
            .collect();
        assert_eq!(thread_a.len(), 2);
        assert!(thread_a.iter().all(|(_, pos)| pos.is_some()));
    }

    #[test]
    fn test_resolve_author_found() {
        let users = sample_users();
        let (handle, name, avatar) = resolve_author("author_1", &users);
        assert_eq!(handle, Some("testuser".to_string()));
        assert_eq!(name, Some("Test User".to_string()));
        assert_eq!(
            avatar,
            Some("https://pbs.twimg.com/photo.jpg".to_string())
        );
    }

    #[test]
    fn test_resolve_author_not_found() {
        let users = sample_users();
        let (handle, name, avatar) = resolve_author("nonexistent", &users);
        assert_eq!(handle, None);
        assert_eq!(name, None);
        assert_eq!(avatar, None);
    }

    #[test]
    fn test_legacy_disposition_is_signal() {
        let bookmark = TwitterBookmark {
            id: "999".to_string(),
            text: "Legacy import tweet.".to_string(),
            note_tweet: None,
            created_at: None,
            author_id: None,
        };

        let fragments = bookmark_to_fragments(&bookmark);
        assert_eq!(fragments[0].disposition, fragment::Disposition::Signal);
    }
}
