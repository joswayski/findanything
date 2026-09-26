use findanything_core::{SearchEngine, instance::Instance, updates};
use serde::Deserialize;
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};
use std::sync::{Mutex, OnceLock};

static ENGINE: OnceLock<Mutex<Option<SearchEngine>>> = OnceLock::new();
static INSTANCE: OnceLock<Mutex<Option<Instance>>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Search { query: String },
    Activate { id: String, query: String },
    UpdatesStatus,
    UpdatesCheck,
    UpdatesApply,
    InstanceAcquire,
    InstancePoll,
}

#[unsafe(no_mangle)]
pub extern "C" fn fa_initialize() {
    updates::initialize();
}

fn request(input: &str) -> Result<Value, String> {
    let operation: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
    match operation {
        Request::Search { query } => {
            let mut engine = ENGINE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|e| e.to_string())?;
            if engine.is_none() {
                *engine = Some(SearchEngine::new()?);
            }
            Ok(json!({"ok":true,"response":engine.as_ref().unwrap().search(&query)}))
        }
        Request::Activate { id, query } => {
            let engine = ENGINE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|e| e.to_string())?;
            engine
                .as_ref()
                .ok_or("Search engine is not initialized")?
                .activate(&id, &query)?;
            Ok(json!({"ok":true}))
        }
        Request::UpdatesStatus => Ok(json!({"ok":true,"status":updates::status()})),
        Request::UpdatesCheck => {
            updates::check();
            Ok(json!({"ok":true}))
        }
        Request::UpdatesApply => {
            updates::apply()?;
            Ok(json!({"ok":true}))
        }
        Request::InstanceAcquire => {
            let mut instance = INSTANCE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|e| e.to_string())?;
            if instance.is_none() {
                *instance = Instance::acquire()?;
            }
            if instance.is_some() {
                updates::start();
            }
            Ok(json!({"ok":true,"primary":instance.is_some()}))
        }
        Request::InstancePoll => {
            let instance = INSTANCE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|e| e.to_string())?;
            Ok(
                json!({"ok":true,"activate":instance.as_ref().is_some_and(Instance::take_activation_request)}),
            )
        }
    }
}

/// # Safety
/// `input` must point to a valid NUL-terminated string for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fa_request(input: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if input.is_null() {
            return Err("Missing request".into());
        }
        // SAFETY: the caller supplies a borrowed, valid C string.
        let input = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|e| e.to_string())?;
        request(input)
    });
    let value = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Native request failed unexpectedly"}),
    };
    // JSON escapes embedded NULs.
    CString::new(value.to_string()).unwrap().into_raw()
}

/// # Safety
/// `value` must be NULL or an unfreed pointer returned by fa_request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fa_string_free(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the caller returns ownership of the original Rust allocation.
        drop(unsafe { CString::from_raw(value) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_requests_return_owned_json_errors() {
        for input in [
            "{}",
            "{\"op\":\"search\"}",
            "{\"op\":\"unknown\"}",
            "not json",
        ] {
            let input = CString::new(input).unwrap();
            unsafe {
                let output = fa_request(input.as_ptr());
                let value: Value =
                    serde_json::from_slice(CStr::from_ptr(output).to_bytes()).unwrap();
                assert_eq!(value["ok"], false);
                assert!(value["error"].is_string());
                fa_string_free(output);
            }
        }
    }
}
