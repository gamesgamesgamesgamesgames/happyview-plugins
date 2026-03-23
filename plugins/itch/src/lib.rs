// Itch.io Plugin for HappyView
// Uses itch.io OAuth2 for authentication

#![cfg_attr(target_arch = "wasm32", no_std)]
#![allow(static_mut_refs)]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
use alloc::{format, string::String, string::ToString, vec, vec::Vec};

#[cfg(target_arch = "wasm32")]
use core::alloc::{GlobalAlloc, Layout};

use serde::{Deserialize, Serialize};

// ============================================================================
// Memory Management (WASM only)
// ============================================================================

#[cfg(target_arch = "wasm32")]
struct BumpAllocator;

#[cfg(target_arch = "wasm32")]
const HEAP_SIZE: usize = 262144; // 256KB

#[cfg(target_arch = "wasm32")]
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[cfg(target_arch = "wasm32")]
static mut HEAP_POS: usize = 0;

#[cfg(target_arch = "wasm32")]
unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let pos = (HEAP_POS + align - 1) & !(align - 1);
        if pos + size > HEAP_SIZE {
            return core::ptr::null_mut();
        }
        HEAP_POS = pos + size;
        HEAP.as_mut_ptr().add(pos)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ============================================================================
// Host Function Imports
// ============================================================================

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn host_http_request(req_ptr: i32, req_len: i32) -> i64;
    fn host_get_secret(name_ptr: i32, name_len: i32) -> i64;
    fn host_log(level_ptr: i32, level_len: i32, msg_ptr: i32, msg_len: i32);
}

// ============================================================================
// Logging Helpers
// ============================================================================

#[cfg(target_arch = "wasm32")]
fn log(level: &str, message: &str) {
    unsafe {
        host_log(
            level.as_ptr() as i32,
            level.len() as i32,
            message.as_ptr() as i32,
            message.len() as i32,
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn log_info(message: &str) {
    log("info", message);
}

#[cfg(target_arch = "wasm32")]
fn log_error(message: &str) {
    log("error", message);
}

#[cfg(not(target_arch = "wasm32"))]
fn log_info(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn log_error(_message: &str) {}

// ============================================================================
// Memory Exports
// ============================================================================

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        let layout = Layout::from_size_align(size as usize, 1).unwrap();
        unsafe { ALLOCATOR.alloc(layout) as u32 }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = size;
        0
    }
}

#[no_mangle]
pub extern "C" fn dealloc(_ptr: u32, _size: u32) {}

// ============================================================================
// Helper Functions
// ============================================================================

fn return_json(s: &str) -> i64 {
    let ptr = alloc(s.len() as u32);
    if ptr == 0 {
        return 0;
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), ptr as *mut u8, s.len());
    }
    ((ptr as i64) << 32) | (s.len() as i64)
}

fn return_ok<T: Serialize>(value: &T) -> i64 {
    let json = serde_json::to_string(&Response::Ok(value)).unwrap_or_default();
    return_json(&json)
}

fn return_error(code: &str, message: &str, retryable: bool) -> i64 {
    let err = ErrorResponse {
        code: code.into(),
        message: message.into(),
        retryable,
    };
    let json = serde_json::to_string(&Response::<()>::Error(err)).unwrap_or_default();
    return_json(&json)
}

#[cfg(target_arch = "wasm32")]
fn read_input(ptr: u32, len: u32) -> Option<Vec<u8>> {
    if len == 0 || len > 1024 * 1024 {
        return None;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    Some(slice.to_vec())
}

#[cfg(target_arch = "wasm32")]
fn read_host_response(packed: i64) -> Option<Vec<u8>> {
    if packed == 0 {
        return None;
    }
    let ptr = (packed >> 32) as u32;
    let len = (packed & 0xFFFFFFFF) as u32;
    if len == 0 || len > 10 * 1024 * 1024 {
        return None;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    Some(slice.to_vec())
}

#[cfg(target_arch = "wasm32")]
fn get_secret(name: &str) -> Option<String> {
    let packed = unsafe { host_get_secret(name.as_ptr() as i32, name.len() as i32) };
    let bytes = read_host_response(packed)?;
    let resp: Response<String> = serde_json::from_slice(&bytes).ok()?;
    match resp {
        Response::Ok(val) => Some(val),
        Response::Error(_) => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn http_post(url: &str, body: &str, content_type: &str) -> Result<String, String> {
    let req = HttpRequest {
        method: "POST".into(),
        url: url.into(),
        headers: vec![("Content-Type".into(), content_type.into())],
        body: Some(body.into()),
    };
    let req_json = serde_json::to_string(&req).map_err(|e| format!("serialize: {}", e))?;
    let packed = unsafe { host_http_request(req_json.as_ptr() as i32, req_json.len() as i32) };
    let bytes = read_host_response(packed).ok_or("no response")?;
    let resp: Response<HttpResponse> =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {}", e))?;
    match resp {
        Response::Ok(r) => r.body.ok_or_else(|| "empty body".into()),
        Response::Error(e) => Err(e.message),
    }
}

#[cfg(target_arch = "wasm32")]
fn http_get_with_auth(url: &str, token: &str) -> Result<String, String> {
    let req = HttpRequest {
        method: "GET".into(),
        url: url.into(),
        headers: vec![("Authorization".into(), format!("Bearer {}", token))],
        body: None,
    };
    let req_json = serde_json::to_string(&req).map_err(|e| format!("serialize: {}", e))?;
    let packed = unsafe { host_http_request(req_json.as_ptr() as i32, req_json.len() as i32) };
    let bytes = read_host_response(packed).ok_or("no response")?;
    let resp: Response<HttpResponse> =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {}", e))?;
    match resp {
        Response::Ok(r) => r.body.ok_or_else(|| "empty body".into()),
        Response::Error(e) => Err(e.message),
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Response<T> {
    Ok(T),
    Error(ErrorResponse),
}

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    code: String,
    message: String,
    retryable: bool,
}

#[derive(Serialize, Deserialize)]
struct PluginInfo {
    id: String,
    name: String,
    version: String,
    api_version: String,
    icon_url: Option<String>,
    required_secrets: Vec<String>,
    auth_type: String,
    config_schema: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct AuthorizeInput {
    state: String,
    redirect_uri: String,
    config: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct CallbackInput {
    code: Option<String>,
    state: String,
    config: serde_json::Value,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct TokenSet {
    access_token: String,
    token_type: String,
    expires_at: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ProfileInput {
    access_token: String,
    config: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct ExternalProfile {
    account_id: String,
    display_name: Option<String>,
    profile_url: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SyncInput {
    access_token: String,
    config: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct SyncRecord {
    collection: String,
    record: serde_json::Value,
    dedup_key: Option<String>,
    sign: bool,
}

#[derive(Serialize, Deserialize)]
struct HttpRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

// itch.io OAuth token response
#[derive(Deserialize)]
struct ItchTokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

// itch.io API types
#[derive(Deserialize)]
struct ItchMeResponse {
    user: ItchUser,
}

#[derive(Deserialize)]
struct ItchUser {
    id: i64,
    username: String,
    display_name: Option<String>,
    cover_url: Option<String>,
    url: Option<String>,
}

#[derive(Deserialize)]
struct ItchOwnedKeysResponse {
    owned_keys: Vec<OwnedKey>,
}

#[derive(Deserialize)]
struct OwnedKey {
    game: OwnedGame,
}

#[derive(Deserialize)]
struct OwnedGame {
    id: i64,
}

// ============================================================================
// Constants
// ============================================================================

const ITCH_AUTHORIZE_URL: &str = "https://itch.io/user/oauth";
const ITCH_TOKEN_URL: &str = "https://itch.io/api/1/oauth/token";
const ITCH_API_BASE: &str = "https://itch.io/api/1";

// ============================================================================
// Plugin Exports
// ============================================================================

#[no_mangle]
pub extern "C" fn plugin_info() -> i64 {
    log_info("itch: plugin_info called");
    let info = PluginInfo {
        id: "itch".into(),
        name: "itch.io".into(),
        version: "0.1.0".into(),
        api_version: "1".into(),
        icon_url: Some("https://itch.io/favicon.ico".into()),
        required_secrets: vec!["CLIENT_ID".into(), "CLIENT_SECRET".into()],
        auth_type: "oauth2".into(),
        config_schema: None,
    };
    return_ok(&info)
}

#[no_mangle]
pub extern "C" fn get_authorize_url(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        log_info("itch: get_authorize_url called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("itch: get_authorize_url failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: AuthorizeInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => {
                log_error(&format!("itch: get_authorize_url parse error: {}", e));
                return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false);
            }
        };

        log_info(&format!("itch: get_authorize_url redirect_uri={}", input.redirect_uri));

        let client_id = match get_secret("CLIENT_ID") {
            Some(id) => id,
            None => {
                log_error("itch: CLIENT_ID not configured");
                return return_error("MISSING_SECRET", "CLIENT_ID not configured", false);
            }
        };

        // itch.io OAuth2 scopes: profile:me, profile:games (owned games)
        let scopes = "profile:me";

        let url = format!(
            "{}?client_id={}&scope={}&response_type=code&redirect_uri={}&state={}",
            ITCH_AUTHORIZE_URL,
            urlencod(&client_id),
            urlencod(scopes),
            urlencod(&input.redirect_uri),
            urlencod(&input.state)
        );

        log_info("itch: get_authorize_url returning URL");
        return_ok(&url)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

#[no_mangle]
pub extern "C" fn handle_callback(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        log_info("itch: handle_callback called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("itch: handle_callback failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: CallbackInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => {
                log_error(&format!("itch: handle_callback parse error: {}", e));
                return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false);
            }
        };

        let code = match &input.code {
            Some(c) => c,
            None => {
                log_error("itch: handle_callback no authorization code received");
                return return_error("MISSING_CODE", "Authorization code is required", false);
            }
        };

        log_info("itch: handle_callback received authorization code");

        let client_id = match get_secret("CLIENT_ID") {
            Some(id) => id,
            None => {
                log_error("itch: CLIENT_ID not configured");
                return return_error("MISSING_SECRET", "CLIENT_ID not configured", false);
            }
        };

        let client_secret = match get_secret("CLIENT_SECRET") {
            Some(s) => s,
            None => {
                log_error("itch: CLIENT_SECRET not configured");
                return return_error("MISSING_SECRET", "CLIENT_SECRET not configured", false);
            }
        };

        // Exchange code for token
        log_info("itch: exchanging code for token");
        let token_body = format!(
            "grant_type=authorization_code&code={}&client_id={}&client_secret={}",
            urlencod(code),
            urlencod(&client_id),
            urlencod(&client_secret)
        );

        let token_response = match http_post(
            ITCH_TOKEN_URL,
            &token_body,
            "application/x-www-form-urlencoded",
        ) {
            Ok(r) => r,
            Err(e) => {
                log_error(&format!("itch: token exchange failed: {}", e));
                return return_error(
                    "TOKEN_ERROR",
                    &format!("Token exchange failed: {}", e),
                    true,
                )
            }
        };

        let tokens: ItchTokenResponse = match serde_json::from_str(&token_response) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("itch: failed to parse token response: {}", e));
                return return_error(
                    "INVALID_RESPONSE",
                    &format!("Failed to parse token: {}", e),
                    false,
                )
            }
        };

        log_info("itch: handle_callback token exchange successful");
        let token_set = TokenSet {
            access_token: tokens.access_token,
            token_type: tokens.token_type,
            expires_at: None, // itch.io tokens don't expire
            refresh_token: tokens.refresh_token,
        };

        return_ok(&token_set)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

#[no_mangle]
pub extern "C" fn refresh_tokens(input_ptr: u32, input_len: u32) -> i64 {
    // itch.io tokens don't expire, so just return the same token
    #[cfg(target_arch = "wasm32")]
    {
        log_info("itch: refresh_tokens called (itch.io tokens don't expire)");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("itch: refresh_tokens failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        #[derive(Deserialize)]
        struct RefreshInput {
            refresh_token: String,
            #[allow(dead_code)]
            config: serde_json::Value,
        }

        let input: RefreshInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => {
                log_error(&format!("itch: refresh_tokens parse error: {}", e));
                return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false);
            }
        };

        // itch.io tokens don't expire - return same token
        log_info("itch: refresh_tokens returning same token (no expiry)");
        let token_set = TokenSet {
            access_token: input.refresh_token,
            token_type: "bearer".into(),
            expires_at: None,
            refresh_token: None,
        };

        return_ok(&token_set)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

#[no_mangle]
pub extern "C" fn get_profile(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        log_info("itch: get_profile called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("itch: get_profile failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: ProfileInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => {
                log_error(&format!("itch: get_profile parse error: {}", e));
                return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false);
            }
        };

        log_info("itch: fetching user profile from /me");
        let url = format!("{}/me", ITCH_API_BASE);
        let body = match http_get_with_auth(&url, &input.access_token) {
            Ok(b) => b,
            Err(e) => {
                log_error(&format!("itch: get_profile HTTP error: {}", e));
                return return_error("HTTP_ERROR", &e, true);
            }
        };

        let me: ItchMeResponse = match serde_json::from_str(&body) {
            Ok(m) => m,
            Err(e) => {
                log_error(&format!("itch: failed to parse profile response: {}", e));
                return return_error("INVALID_RESPONSE", &format!("Parse error: {}", e), false)
            }
        };

        log_info(&format!("itch: get_profile successful for user_id={} username={}", me.user.id, me.user.username));
        let profile = ExternalProfile {
            account_id: me.user.id.to_string(),
            display_name: me.user.display_name.or(Some(me.user.username)),
            profile_url: me.user.url,
            avatar_url: me.user.cover_url,
        };

        return_ok(&profile)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

#[no_mangle]
pub extern "C" fn sync_account(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        log_info("itch: sync_account called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("itch: sync_account failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: SyncInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => {
                log_error(&format!("itch: sync_account parse error: {}", e));
                return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false);
            }
        };

        // Fetch user profile to validate token
        log_info("itch: validating token with /me");
        let me_url = format!("{}/me", ITCH_API_BASE);
        let me_body = match http_get_with_auth(&me_url, &input.access_token) {
            Ok(b) => b,
            Err(e) => {
                log_error(&format!("itch: sync_account token validation failed: {}", e));
                return return_error("HTTP_ERROR", &e, true);
            }
        };

        // Validate token by parsing profile response
        if let Err(e) = serde_json::from_str::<ItchMeResponse>(&me_body) {
            log_error(&format!("itch: sync_account profile parse error: {}", e));
            return return_error("INVALID_RESPONSE", &format!("Parse error: {}", e), false);
        }

        log_info("itch: token validated, fetching owned games");

        // Fetch owned games - paginated
        let mut all_games: Vec<OwnedKey> = Vec::new();
        let mut page = 1;

        loop {
            log_info(&format!("itch: fetching owned keys page {}", page));
            let keys_url = format!("{}/my-owned-keys?page={}", ITCH_API_BASE, page);
            let keys_body = match http_get_with_auth(&keys_url, &input.access_token) {
                Ok(b) => b,
                Err(e) => {
                    log_info(&format!("itch: stopping pagination at page {} due to error: {}", page, e));
                    break;
                }
            };

            let keys: ItchOwnedKeysResponse = match serde_json::from_str(&keys_body) {
                Ok(k) => k,
                Err(e) => {
                    log_info(&format!("itch: stopping pagination at page {} due to parse error: {}", page, e));
                    break;
                }
            };

            if keys.owned_keys.is_empty() {
                log_info(&format!("itch: no more games on page {}, ending pagination", page));
                break;
            }

            log_info(&format!("itch: fetched {} games on page {}", keys.owned_keys.len(), page));
            all_games.extend(keys.owned_keys);
            page += 1;

            // Safety limit
            if page > 50 {
                log_info("itch: reached page limit (50), ending pagination");
                break;
            }
        }

        log_info(&format!("itch: fetched {} total games", all_games.len()));

        // Build sync records
        let mut records: Vec<SyncRecord> = Vec::new();

        // Game ownership records
        for key in all_games {
            let game_record = serde_json::json!({
                "$type": "games.gamesgamesgamesgames.actor.game",
                "game": {
                    "platform": "itch",
                    "externalId": key.game.id.to_string(),
                },
                "platform": "itch",
                "createdAt": chrono_now(),
            });

            records.push(SyncRecord {
                collection: "games.gamesgamesgamesgames.actor.game".into(),
                record: game_record,
                dedup_key: Some(format!("itch:game:{}", key.game.id)),
                sign: true,
            });
        }

        log_info(&format!("itch: sync_account completed with {} records", records.len()));
        return_ok(&records)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

fn urlencod(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}

fn chrono_now() -> String {
    "2024-01-01T00:00:00Z".into()
}
