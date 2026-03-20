// PlayStation Plugin for HappyView
// Uses PlayStation Network OAuth2 for authentication

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
}

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

// PSN OAuth token response
#[derive(Deserialize)]
struct PsnTokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

// PSN API types
#[derive(Deserialize)]
struct PsnProfile {
    #[serde(rename = "onlineId")]
    online_id: String,
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct PsnTitlesResponse {
    titles: Vec<PsnTitle>,
}

#[derive(Deserialize)]
struct PsnTitle {
    #[serde(rename = "titleId")]
    title_id: String,
    name: String,
    #[serde(rename = "imageUrl")]
    image_url: Option<String>,
    #[serde(rename = "playDuration")]
    play_duration: Option<String>,
}

// ============================================================================
// Constants
// ============================================================================

// PSN uses the mobile app's OAuth client (public client, no secret needed)
const PSN_CLIENT_ID: &str = "ac8d161a-d966-4728-b0ea-ffec22f69edc";
const PSN_AUTHORIZE_URL: &str = "https://ca.account.sony.com/api/authz/v3/oauth/authorize";
const PSN_TOKEN_URL: &str = "https://ca.account.sony.com/api/authz/v3/oauth/token";
const PSN_API_BASE: &str = "https://m.np.playstation.com/api";

// ============================================================================
// Plugin Exports
// ============================================================================

#[no_mangle]
pub extern "C" fn plugin_info() -> i64 {
    let info = PluginInfo {
        id: "playstation".into(),
        name: "PlayStation".into(),
        version: "0.1.0".into(),
        api_version: "1".into(),
        icon_url: Some("https://www.playstation.com/favicon.ico".into()),
        // No secrets needed - uses public PSN client ID
        required_secrets: vec![],
        auth_type: "oauth2".into(),
        config_schema: None,
    };
    return_ok(&info)
}

#[no_mangle]
pub extern "C" fn get_authorize_url(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(bytes) = read_input(input_ptr, input_len) else {
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: AuthorizeInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false),
        };

        // PSN OAuth2 with PKCE (using mobile app client)
        let scope = "psn:mobile.v2.core psn:clientapp";

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            PSN_AUTHORIZE_URL,
            urlencod(PSN_CLIENT_ID),
            urlencod(&input.redirect_uri),
            urlencod(scope),
            urlencod(&input.state)
        );

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
        let Some(bytes) = read_input(input_ptr, input_len) else {
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: CallbackInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false),
        };

        let code = match &input.code {
            Some(c) => c,
            None => return return_error("MISSING_CODE", "Authorization code is required", false),
        };

        // Exchange code for token (public client, no secret)
        let token_body = format!(
            "grant_type=authorization_code&code={}&client_id={}",
            urlencod(code),
            urlencod(PSN_CLIENT_ID)
        );

        let token_response = match http_post(
            PSN_TOKEN_URL,
            &token_body,
            "application/x-www-form-urlencoded",
        ) {
            Ok(r) => r,
            Err(e) => return return_error("TOKEN_ERROR", &format!("Token exchange failed: {}", e), true),
        };

        let tokens: PsnTokenResponse = match serde_json::from_str(&token_response) {
            Ok(t) => t,
            Err(e) => return return_error("INVALID_RESPONSE", &format!("Failed to parse token: {}", e), false),
        };

        let token_set = TokenSet {
            access_token: tokens.access_token,
            token_type: tokens.token_type,
            expires_at: None,
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
    #[cfg(target_arch = "wasm32")]
    {
        let Some(bytes) = read_input(input_ptr, input_len) else {
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
            Err(e) => return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false),
        };

        let token_body = format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            urlencod(&input.refresh_token),
            urlencod(PSN_CLIENT_ID)
        );

        let token_response = match http_post(
            PSN_TOKEN_URL,
            &token_body,
            "application/x-www-form-urlencoded",
        ) {
            Ok(r) => r,
            Err(e) => return return_error("TOKEN_ERROR", &format!("Token refresh failed: {}", e), true),
        };

        let tokens: PsnTokenResponse = match serde_json::from_str(&token_response) {
            Ok(t) => t,
            Err(e) => return return_error("INVALID_RESPONSE", &format!("Failed to parse token: {}", e), false),
        };

        let token_set = TokenSet {
            access_token: tokens.access_token,
            token_type: tokens.token_type,
            expires_at: None,
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
pub extern "C" fn get_profile(input_ptr: u32, input_len: u32) -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(bytes) = read_input(input_ptr, input_len) else {
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: ProfileInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false),
        };

        let url = format!("{}/userProfile/v1/internal/users/me/profiles", PSN_API_BASE);
        let body = match http_get_with_auth(&url, &input.access_token) {
            Ok(b) => b,
            Err(e) => return return_error("HTTP_ERROR", &e, true),
        };

        let psn_profile: PsnProfile = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => return return_error("INVALID_RESPONSE", &format!("Parse error: {}", e), false),
        };

        let profile = ExternalProfile {
            account_id: psn_profile.account_id,
            display_name: Some(psn_profile.online_id),
            profile_url: None,
            avatar_url: psn_profile.avatar_url,
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
        let Some(bytes) = read_input(input_ptr, input_len) else {
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let input: SyncInput = match serde_json::from_slice(&bytes) {
            Ok(i) => i,
            Err(e) => return return_error("INVALID_INPUT", &format!("Parse error: {}", e), false),
        };

        // Fetch played titles
        let titles_url = format!("{}/gamelist/v2/users/me/titles", PSN_API_BASE);
        let titles_body = match http_get_with_auth(&titles_url, &input.access_token) {
            Ok(b) => b,
            Err(e) => return return_error("HTTP_ERROR", &e, true),
        };

        let titles: PsnTitlesResponse = match serde_json::from_str(&titles_body) {
            Ok(t) => t,
            Err(e) => return return_error("INVALID_RESPONSE", &format!("Parse error: {}", e), false),
        };

        // Build sync records
        let mut records: Vec<SyncRecord> = Vec::new();

        for title in titles.titles {
            let game_record = serde_json::json!({
                "$type": "games.gamesgamesgamesgames.actor.game",
                "game": {
                    "platform": "playstation",
                    "externalId": title.title_id,
                },
                "platform": "playstation",
                "createdAt": chrono_now(),
            });

            records.push(SyncRecord {
                collection: "games.gamesgamesgamesgames.actor.game".into(),
                record: game_record,
                dedup_key: Some(format!("playstation:game:{}", title.title_id)),
                sign: true,
            });
        }

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
