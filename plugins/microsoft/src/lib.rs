// Microsoft Plugin for HappyView
// Uses Microsoft OAuth2 + Microsoft Graph API for authentication and data

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
const HEAP_SIZE: usize = 131072; // 128KB

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

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No-op for bump allocator
    }
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
pub extern "C" fn dealloc(_ptr: u32, _size: u32) {
    // No-op for bump allocator
}

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
    String::from_utf8(bytes).ok()
}

#[cfg(target_arch = "wasm32")]
fn http_post(url: &str, body: &str, content_type: &str) -> Result<String, String> {
    let req = serde_json::json!({
        "method": "POST",
        "url": url,
        "headers": [["Content-Type", content_type]],
        "body": body.as_bytes().to_vec()
    });
    let req_str = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    let packed = unsafe { host_http_request(req_str.as_ptr() as i32, req_str.len() as i32) };
    let bytes = read_host_response(packed).ok_or("No response")?;
    let resp: HostHttpResponse =
        serde_json::from_slice(&bytes).map_err(|e| format!("Parse error: {}", e))?;

    match resp {
        HostHttpResponse::Ok { ok } => {
            if ok.status >= 200 && ok.status < 300 {
                String::from_utf8(ok.body).map_err(|_| "Invalid UTF-8".to_string())
            } else {
                let body_str = String::from_utf8_lossy(&ok.body);
                Err(format!("HTTP {}: {}", ok.status, body_str))
            }
        }
        HostHttpResponse::Error { error } => Err(error.message),
    }
}

#[cfg(target_arch = "wasm32")]
fn http_get_with_auth(url: &str, token: &str) -> Result<String, String> {
    let req = serde_json::json!({
        "method": "GET",
        "url": url,
        "headers": [
            ["Authorization", format!("Bearer {}", token)]
        ],
        "body": null
    });
    let req_str = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    let packed = unsafe { host_http_request(req_str.as_ptr() as i32, req_str.len() as i32) };
    let bytes = read_host_response(packed).ok_or("No response")?;
    let resp: HostHttpResponse =
        serde_json::from_slice(&bytes).map_err(|e| format!("Parse error: {}", e))?;

    match resp {
        HostHttpResponse::Ok { ok } => {
            if ok.status >= 200 && ok.status < 300 {
                String::from_utf8(ok.body).map_err(|_| "Invalid UTF-8".to_string())
            } else {
                Err(format!("HTTP {}", ok.status))
            }
        }
        HostHttpResponse::Error { error } => Err(error.message),
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

// Host HTTP response types
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum HostHttpResponse {
    Ok { ok: HttpResponseBody },
    Error { error: HostError },
}

#[derive(Deserialize)]
struct HttpResponseBody {
    status: u16,
    #[allow(dead_code)]
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Deserialize)]
struct HostError {
    #[allow(dead_code)]
    code: String,
    message: String,
}

// Microsoft OAuth response
#[derive(Deserialize)]
struct MsTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    #[allow(dead_code)]
    token_type: String,
}

// Microsoft Graph profile response
#[derive(Deserialize)]
struct GraphUserProfile {
    id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "userPrincipalName")]
    user_principal_name: Option<String>,
}

// ============================================================================
// Plugin Exports
// ============================================================================

#[no_mangle]
pub extern "C" fn plugin_info() -> i64 {
    log_info("microsoft: plugin_info called");
    let info = PluginInfo {
        id: "microsoft".into(),
        name: "Microsoft".into(),
        version: "0.1.0".into(),
        api_version: "1".into(),
        icon_url: None,
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
        log_info("microsoft: get_authorize_url called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("microsoft: get_authorize_url failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<AuthorizeInput>(&bytes) else {
            log_error("microsoft: get_authorize_url failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        log_info(&format!("microsoft: get_authorize_url redirect_uri={}", input.redirect_uri));

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("microsoft: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        // Microsoft Graph scopes for user profile
        let scopes = "User.Read offline_access";
        let url = format!(
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}",
            urlencoding_encode(&client_id),
            urlencoding_encode(&input.redirect_uri),
            urlencoding_encode(scopes),
            urlencoding_encode(&input.state)
        );

        log_info("microsoft: get_authorize_url returning URL");
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
        log_info("microsoft: handle_callback called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("microsoft: handle_callback failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<CallbackInput>(&bytes) else {
            log_error("microsoft: handle_callback failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        let Some(code) = input.code else {
            log_error("microsoft: handle_callback no authorization code received");
            return return_error("AUTH_FAILED", "No authorization code received", false);
        };

        log_info("microsoft: handle_callback received authorization code");

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("microsoft: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        let Some(client_secret) = get_secret("CLIENT_SECRET") else {
            log_error("microsoft: CLIENT_SECRET not configured");
            return return_error("CONFIG_ERROR", "CLIENT_SECRET not configured", false);
        };

        let redirect_uri = input
            .extra
            .get("redirect_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:3001/dashboard/settings/accounts/");

        log_info(&format!("microsoft: handle_callback redirect_uri={}", redirect_uri));

        // Exchange code for Microsoft token
        log_info("microsoft: exchanging code for token");
        let token_url = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
        let body = format!(
            "client_id={}&client_secret={}&code={}&redirect_uri={}&grant_type=authorization_code",
            urlencoding_encode(&client_id),
            urlencoding_encode(&client_secret),
            urlencoding_encode(&code),
            urlencoding_encode(redirect_uri)
        );

        let ms_token_resp = match http_post(token_url, &body, "application/x-www-form-urlencoded") {
            Ok(r) => r,
            Err(e) => {
                log_error(&format!("microsoft: token exchange failed: {}", e));
                return return_error("TOKEN_ERROR", &format!("Failed to get token: {}", e), true)
            }
        };

        let ms_token: MsTokenResponse = match serde_json::from_str(&ms_token_resp) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("microsoft: failed to parse token response: {}", e));
                return return_error(
                    "TOKEN_ERROR",
                    &format!("Failed to parse token: {}", e),
                    false,
                )
            }
        };

        log_info("microsoft: handle_callback token exchange successful");
        let token_set = TokenSet {
            access_token: ms_token.access_token,
            token_type: "Bearer".into(),
            expires_at: ms_token.expires_in.map(|s| format!("{}s", s)),
            refresh_token: ms_token.refresh_token,
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
        log_info("microsoft: refresh_tokens called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("microsoft: refresh_tokens failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        #[derive(Deserialize)]
        struct RefreshInput {
            refresh_token: String,
            #[allow(dead_code)]
            config: serde_json::Value,
        }

        let Ok(input) = serde_json::from_slice::<RefreshInput>(&bytes) else {
            log_error("microsoft: refresh_tokens failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("microsoft: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        let Some(client_secret) = get_secret("CLIENT_SECRET") else {
            log_error("microsoft: CLIENT_SECRET not configured");
            return return_error("CONFIG_ERROR", "CLIENT_SECRET not configured", false);
        };

        log_info("microsoft: refreshing token");
        let token_url = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
        let body = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            urlencoding_encode(&client_id),
            urlencoding_encode(&client_secret),
            urlencoding_encode(&input.refresh_token)
        );

        let resp = match http_post(token_url, &body, "application/x-www-form-urlencoded") {
            Ok(r) => r,
            Err(e) => {
                log_error(&format!("microsoft: token refresh failed: {}", e));
                return return_error("TOKEN_ERROR", &format!("Failed to refresh: {}", e), true)
            }
        };

        let ms_token: MsTokenResponse = match serde_json::from_str(&resp) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("microsoft: failed to parse refresh response: {}", e));
                return return_error("TOKEN_ERROR", &format!("Failed to parse: {}", e), false)
            }
        };

        log_info("microsoft: refresh_tokens successful");
        let token_set = TokenSet {
            access_token: ms_token.access_token,
            token_type: "Bearer".into(),
            expires_at: ms_token.expires_in.map(|s| format!("{}s", s)),
            refresh_token: ms_token.refresh_token,
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
        log_info("microsoft: get_profile called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("microsoft: get_profile failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<ProfileInput>(&bytes) else {
            log_error("microsoft: get_profile failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        // Get user profile from Microsoft Graph
        log_info("microsoft: fetching profile from Graph API");
        let profile_url = "https://graph.microsoft.com/v1.0/me";
        let graph_profile = match http_get_with_auth(profile_url, &input.access_token) {
            Ok(resp) => match serde_json::from_str::<GraphUserProfile>(&resp) {
                Ok(p) => p,
                Err(e) => {
                    log_error(&format!("microsoft: failed to parse profile: {}", e));
                    return return_error(
                        "PROFILE_ERROR",
                        &format!("Failed to parse profile: {}", e),
                        false,
                    )
                }
            },
            Err(e) => {
                log_error(&format!("microsoft: failed to get profile: {}", e));
                return return_error(
                    "PROFILE_ERROR",
                    &format!("Failed to get profile: {}", e),
                    true,
                )
            }
        };

        log_info(&format!("microsoft: get_profile successful for id={}", graph_profile.id));
        let profile = ExternalProfile {
            account_id: graph_profile.id,
            display_name: graph_profile.display_name,
            profile_url: Some("https://account.microsoft.com/profile".into()),
            avatar_url: None, // Microsoft Graph requires separate call for photo
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
        log_info("microsoft: sync_account called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("microsoft: sync_account failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(_input) = serde_json::from_slice::<SyncInput>(&bytes) else {
            log_error("microsoft: sync_account failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        // TODO: What Microsoft data should we sync?
        // Options:
        // - Microsoft Store purchases (requires different API/scopes)
        // - PC Game Pass data (requires Xbox APIs, which xbox plugin handles)
        // For now, return empty - this plugin is primarily for account linking
        log_info("microsoft: sync_account returning empty (account linking only)");
        let records: Vec<SyncRecord> = vec![];

        return_ok(&records)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

// ============================================================================
// URL Encoding (minimal implementation for no_std)
// ============================================================================

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push('%');
                    result.push_str(&format!("{:02X}", b));
                }
            }
        }
    }
    result
}
