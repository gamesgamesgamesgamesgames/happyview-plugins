// Xbox Plugin for HappyView
// Uses Microsoft OAuth2 + Xbox Live API for authentication and data

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
            ["Authorization", token],
            ["x-xbl-contract-version", "2"]
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

// Xbox Live auth response
#[derive(Serialize, Deserialize)]
struct XblAuthRequest {
    #[serde(rename = "RelyingParty")]
    relying_party: String,
    #[serde(rename = "TokenType")]
    token_type: String,
    #[serde(rename = "Properties")]
    properties: XblAuthProperties,
}

#[derive(Serialize, Deserialize)]
struct XblAuthProperties {
    #[serde(rename = "AuthMethod")]
    auth_method: String,
    #[serde(rename = "SiteName")]
    site_name: String,
    #[serde(rename = "RpsTicket")]
    rps_ticket: String,
}

#[derive(Deserialize)]
struct XblAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblUserInfo>,
}

#[derive(Deserialize)]
struct XblUserInfo {
    uhs: String,
    #[allow(dead_code)]
    xid: Option<String>,
    #[allow(dead_code)]
    gtg: Option<String>,
}

// XSTS auth
#[derive(Serialize)]
struct XstsAuthRequest {
    #[serde(rename = "RelyingParty")]
    relying_party: String,
    #[serde(rename = "TokenType")]
    token_type: String,
    #[serde(rename = "Properties")]
    properties: XstsAuthProperties,
}

#[derive(Serialize)]
struct XstsAuthProperties {
    #[serde(rename = "SandboxId")]
    sandbox_id: String,
    #[serde(rename = "UserTokens")]
    user_tokens: Vec<String>,
}

#[derive(Deserialize)]
struct XstsAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XstsDisplayClaims,
}

#[derive(Deserialize)]
struct XstsDisplayClaims {
    xui: Vec<XstsUserInfo>,
}

#[derive(Deserialize)]
struct XstsUserInfo {
    #[allow(dead_code)]
    uhs: String,
    xid: String,
    gtg: String,
}

// Xbox profile response
#[derive(Deserialize)]
struct XboxProfileResponse {
    #[serde(rename = "profileUsers")]
    profile_users: Vec<XboxProfileUser>,
}

#[derive(Deserialize)]
struct XboxProfileUser {
    settings: Vec<XboxProfileSetting>,
}

#[derive(Deserialize)]
struct XboxProfileSetting {
    id: String,
    value: String,
}

// Xbox title history
#[derive(Deserialize)]
struct TitleHistoryResponse {
    titles: Vec<XboxTitle>,
}

#[derive(Deserialize)]
struct XboxTitle {
    #[serde(rename = "titleId")]
    title_id: String,
    name: String,
    #[serde(rename = "modernTitleId")]
    modern_title_id: Option<String>,
    #[serde(rename = "titleHistory")]
    title_history: Option<TitleHistory>,
}

#[derive(Deserialize)]
struct TitleHistory {
    #[serde(rename = "lastTimePlayed")]
    last_time_played: String,
}

// ============================================================================
// Plugin Exports
// ============================================================================

#[no_mangle]
pub extern "C" fn plugin_info() -> i64 {
    log_info("xbox: plugin_info called");
    let info = PluginInfo {
        id: "xbox".into(),
        name: "Xbox".into(),
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
        log_info("xbox: get_authorize_url called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("xbox: get_authorize_url failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<AuthorizeInput>(&bytes) else {
            log_error("xbox: get_authorize_url failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        log_info(&format!(
            "xbox: get_authorize_url redirect_uri={}",
            input.redirect_uri
        ));

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("xbox: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        // Microsoft OAuth2 authorization URL
        let scopes = "XboxLive.signin XboxLive.offline_access";
        let url = format!(
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}",
            urlencoding_encode(&client_id),
            urlencoding_encode(&input.redirect_uri),
            urlencoding_encode(scopes),
            urlencoding_encode(&input.state)
        );

        log_info("xbox: get_authorize_url returning URL");
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
        log_info("xbox: handle_callback called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("xbox: handle_callback failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<CallbackInput>(&bytes) else {
            log_error("xbox: handle_callback failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        let Some(code) = input.code else {
            log_error("xbox: handle_callback no authorization code received");
            return return_error("AUTH_FAILED", "No authorization code received", false);
        };

        log_info("xbox: handle_callback received authorization code");

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("xbox: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        let Some(client_secret) = get_secret("CLIENT_SECRET") else {
            log_error("xbox: CLIENT_SECRET not configured");
            return return_error("CONFIG_ERROR", "CLIENT_SECRET not configured", false);
        };

        // Get redirect_uri from the extra params (passed by HappyView)
        let redirect_uri = input
            .extra
            .get("redirect_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:3001/dashboard/settings/accounts/");

        log_info(&format!(
            "xbox: handle_callback redirect_uri={}",
            redirect_uri
        ));

        // Exchange code for Microsoft token
        log_info("xbox: exchanging code for Microsoft token");
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
                log_error(&format!("xbox: MS token exchange failed: {}", e));
                return return_error(
                    "TOKEN_ERROR",
                    &format!("Failed to get MS token: {}", e),
                    true,
                );
            }
        };

        let ms_token: MsTokenResponse = match serde_json::from_str(&ms_token_resp) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: failed to parse MS token response: {}", e));
                return return_error(
                    "TOKEN_ERROR",
                    &format!("Failed to parse MS token: {}", e),
                    false,
                );
            }
        };

        log_info("xbox: handle_callback MS token exchange successful");

        // The MS access token is used as the "access_token" for simplicity
        // In get_profile and sync_account, we'll exchange it for Xbox Live tokens
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
        log_info("xbox: refresh_tokens called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("xbox: refresh_tokens failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        #[derive(Deserialize)]
        struct RefreshInput {
            refresh_token: String,
            #[allow(dead_code)]
            config: serde_json::Value,
        }

        let Ok(input) = serde_json::from_slice::<RefreshInput>(&bytes) else {
            log_error("xbox: refresh_tokens failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        let Some(client_id) = get_secret("CLIENT_ID") else {
            log_error("xbox: CLIENT_ID not configured");
            return return_error("CONFIG_ERROR", "CLIENT_ID not configured", false);
        };

        let Some(client_secret) = get_secret("CLIENT_SECRET") else {
            log_error("xbox: CLIENT_SECRET not configured");
            return return_error("CONFIG_ERROR", "CLIENT_SECRET not configured", false);
        };

        log_info("xbox: refreshing MS token");
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
                log_error(&format!("xbox: token refresh failed: {}", e));
                return return_error("TOKEN_ERROR", &format!("Failed to refresh: {}", e), true);
            }
        };

        let ms_token: MsTokenResponse = match serde_json::from_str(&resp) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: failed to parse refresh response: {}", e));
                return return_error("TOKEN_ERROR", &format!("Failed to parse: {}", e), false);
            }
        };

        log_info("xbox: refresh_tokens successful");
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
        log_info("xbox: get_profile called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("xbox: get_profile failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<ProfileInput>(&bytes) else {
            log_error("xbox: get_profile failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        // Exchange MS token for Xbox Live token
        log_info("xbox: exchanging MS token for XBL token");
        let (xbl_token, user_hash) = match get_xbl_token(&input.access_token) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: XBL token exchange failed: {}", e));
                return return_error("AUTH_ERROR", &e, true);
            }
        };

        // Exchange XBL token for XSTS token
        log_info("xbox: exchanging XBL token for XSTS token");
        let (xsts_token, xuid, gamertag) = match get_xsts_token(&xbl_token) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: XSTS token exchange failed: {}", e));
                return return_error("AUTH_ERROR", &e, true);
            }
        };

        log_info(&format!(
            "xbox: got XSTS token for xuid={} gamertag={}",
            xuid, gamertag
        ));

        // Get profile details
        log_info("xbox: fetching profile details");
        let auth_header = format!("XBL3.0 x={};{}", user_hash, xsts_token);
        let profile_url = format!(
            "https://profile.xboxlive.com/users/xuid({})/profile/settings?settings=Gamertag,GameDisplayPicRaw",
            xuid
        );

        let (display_name, avatar_url) = match http_get_with_auth(&profile_url, &auth_header) {
            Ok(resp) => {
                if let Ok(profile_resp) = serde_json::from_str::<XboxProfileResponse>(&resp) {
                    let mut name = gamertag.clone();
                    let mut avatar = None;
                    if let Some(user) = profile_resp.profile_users.first() {
                        for setting in &user.settings {
                            if setting.id == "Gamertag" {
                                name = setting.value.clone();
                            } else if setting.id == "GameDisplayPicRaw" {
                                avatar = Some(setting.value.clone());
                            }
                        }
                    }
                    log_info(&format!("xbox: profile fetched for {}", name));
                    (name, avatar)
                } else {
                    log_info("xbox: using gamertag from XSTS (profile parse failed)");
                    (gamertag.clone(), None)
                }
            }
            Err(e) => {
                log_info(&format!(
                    "xbox: profile fetch failed ({}), using gamertag from XSTS",
                    e
                ));
                (gamertag.clone(), None)
            }
        };

        let profile = ExternalProfile {
            account_id: xuid,
            display_name: Some(display_name.clone()),
            profile_url: Some(format!(
                "https://www.xbox.com/en-US/play/user/{}",
                display_name
            )),
            avatar_url,
        };

        log_info("xbox: get_profile completed successfully");
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
        log_info("xbox: sync_account called");

        let Some(bytes) = read_input(input_ptr, input_len) else {
            log_error("xbox: sync_account failed to read input");
            return return_error("INVALID_INPUT", "Failed to read input", false);
        };

        let Ok(input) = serde_json::from_slice::<SyncInput>(&bytes) else {
            log_error("xbox: sync_account failed to parse input");
            return return_error("INVALID_INPUT", "Failed to parse input", false);
        };

        // Exchange MS token for Xbox Live tokens
        log_info("xbox: exchanging MS token for XBL token");
        let (xbl_token, user_hash) = match get_xbl_token(&input.access_token) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: XBL token exchange failed: {}", e));
                return return_error("AUTH_ERROR", &e, true);
            }
        };

        log_info("xbox: exchanging XBL token for XSTS token");
        let (xsts_token, xuid, _gamertag) = match get_xsts_token(&xbl_token) {
            Ok(t) => t,
            Err(e) => {
                log_error(&format!("xbox: XSTS token exchange failed: {}", e));
                return return_error("AUTH_ERROR", &e, true);
            }
        };

        log_info(&format!("xbox: fetching title history for xuid={}", xuid));
        let auth_header = format!("XBL3.0 x={};{}", user_hash, xsts_token);

        // Get title history (games played)
        let titles_url = format!(
            "https://titlehub.xboxlive.com/users/xuid({})/titles/titlehistory/decoration/detail",
            xuid
        );

        let titles = match http_get_with_auth(&titles_url, &auth_header) {
            Ok(resp) => match serde_json::from_str::<TitleHistoryResponse>(&resp) {
                Ok(t) => {
                    log_info(&format!("xbox: fetched {} titles", t.titles.len()));
                    t.titles
                }
                Err(e) => {
                    log_error(&format!("xbox: failed to parse title history: {}", e));
                    vec![]
                }
            },
            Err(e) => {
                log_error(&format!("xbox: failed to fetch title history: {}", e));
                vec![]
            }
        };

        // Convert to sync records
        let mut records = Vec::new();
        let now = "2026-03-20T00:00:00Z"; // TODO: get actual time from host

        for title in titles {
            let record = serde_json::json!({
                "$type": "games.gamesgamesgamesgames.actor.game",
                "game": {
                    "platform": "xbox",
                    "externalId": title.title_id
                },
                "platform": "xbox",
                "createdAt": now,
                "lastPlayedAt": title.title_history.as_ref().map(|h| &h.last_time_played),
                "externalData": {
                    "name": title.name,
                    "modernTitleId": title.modern_title_id
                }
            });

            records.push(SyncRecord {
                collection: "games.gamesgamesgamesgames.actor.game".into(),
                record,
                dedup_key: Some(format!("xbox:game:{}", title.title_id)),
                sign: true,
            });
        }

        log_info(&format!(
            "xbox: sync_account completed with {} records",
            records.len()
        ));
        return_ok(&records)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (input_ptr, input_len);
        return_error("NOT_WASM", "Only runs in WASM", false)
    }
}

// ============================================================================
// Xbox Live Token Exchange Helpers
// ============================================================================

#[cfg(target_arch = "wasm32")]
fn get_xbl_token(ms_access_token: &str) -> Result<(String, String), String> {
    log_info("xbox: get_xbl_token - authenticating with Xbox Live");
    let xbl_url = "https://user.auth.xboxlive.com/user/authenticate";

    let req = XblAuthRequest {
        relying_party: "http://auth.xboxlive.com".into(),
        token_type: "JWT".into(),
        properties: XblAuthProperties {
            auth_method: "RPS".into(),
            site_name: "user.auth.xboxlive.com".into(),
            rps_ticket: format!("d={}", ms_access_token),
        },
    };

    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    let resp = http_post(xbl_url, &body, "application/json").map_err(|e| {
        log_error(&format!("xbox: XBL auth request failed: {}", e));
        e
    })?;

    let xbl_resp: XblAuthResponse = serde_json::from_str(&resp).map_err(|e| {
        log_error(&format!("xbox: failed to parse XBL response: {}", e));
        format!("Failed to parse XBL response: {}", e)
    })?;

    let user_hash = xbl_resp
        .display_claims
        .xui
        .first()
        .map(|u| u.uhs.clone())
        .ok_or_else(|| {
            log_error("xbox: no user hash in XBL response");
            "No user hash in XBL response".to_string()
        })?;

    log_info("xbox: get_xbl_token successful");
    Ok((xbl_resp.token, user_hash))
}

#[cfg(target_arch = "wasm32")]
fn get_xsts_token(xbl_token: &str) -> Result<(String, String, String), String> {
    log_info("xbox: get_xsts_token - getting XSTS token");
    let xsts_url = "https://xsts.auth.xboxlive.com/xsts/authorize";

    let req = XstsAuthRequest {
        relying_party: "http://xboxlive.com".into(),
        token_type: "JWT".into(),
        properties: XstsAuthProperties {
            sandbox_id: "RETAIL".into(),
            user_tokens: vec![xbl_token.to_string()],
        },
    };

    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    let resp = http_post(xsts_url, &body, "application/json").map_err(|e| {
        log_error(&format!("xbox: XSTS auth request failed: {}", e));
        e
    })?;

    let xsts_resp: XstsAuthResponse = serde_json::from_str(&resp).map_err(|e| {
        log_error(&format!("xbox: failed to parse XSTS response: {}", e));
        format!("Failed to parse XSTS response: {}", e)
    })?;

    let user_info = xsts_resp.display_claims.xui.first().ok_or_else(|| {
        log_error("xbox: no user info in XSTS response");
        "No user info in XSTS response".to_string()
    })?;

    log_info(&format!(
        "xbox: get_xsts_token successful for xid={}",
        user_info.xid
    ));
    Ok((
        xsts_resp.token,
        user_info.xid.clone(),
        user_info.gtg.clone(),
    ))
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
