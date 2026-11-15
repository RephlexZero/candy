//! Browser plumbing: local storage, file open/save, clipboard and fetch.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn get(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}

pub fn set(key: &str, value: &str) -> bool {
    storage().is_some_and(|s| s.set_item(key, value).is_ok())
}

#[wasm_bindgen(inline_js = r#"
let handle = null;
export function fs_supported() { return typeof window.showSaveFilePicker === 'function'; }
export function fs_clear() { handle = null; }
export function fs_has_handle() { return handle !== null; }
export async function fs_open() {
  const [h] = await window.showOpenFilePicker({
    types: [{ description: 'CAN database', accept: { 'text/plain': ['.dbc'] } }],
  });
  handle = h;
  const file = await h.getFile();
  return [file.name, await file.text()];
}
export async function fs_save(name, text, saveAs) {
  if (!handle || saveAs) {
    handle = await window.showSaveFilePicker({
      suggestedName: name,
      types: [{ description: 'CAN database', accept: { 'text/plain': ['.dbc'] } }],
    });
  }
  const w = await handle.createWritable();
  await w.write(text);
  await w.close();
  return handle.name;
}
export async function file_text(file) { return await file.text(); }
"#)]
extern "C" {
    fn fs_supported() -> bool;
    fn fs_clear();
    fn fs_has_handle() -> bool;
    #[wasm_bindgen(catch)]
    async fn fs_open() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn fs_save(name: &str, text: &str, save_as: bool) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn file_text(file: &web_sys::File) -> Result<JsValue, JsValue>;
}

pub fn native_fs() -> bool {
    fs_supported()
}

pub fn forget_handle() {
    fs_clear();
}

fn is_abort(e: &JsValue) -> bool {
    js_sys::Reflect::get(e, &"name".into())
        .ok()
        .and_then(|n| n.as_string())
        .as_deref()
        == Some("AbortError")
}

/// Open a file with the File System Access API. `Ok(None)` when the user cancelled.
pub async fn open_native() -> Result<Option<(String, String)>, String> {
    match fs_open().await {
        Ok(v) => {
            let arr: js_sys::Array = v.unchecked_into();
            Ok(Some((
                arr.get(0).as_string().unwrap_or_default(),
                arr.get(1).as_string().unwrap_or_default(),
            )))
        }
        Err(e) if is_abort(&e) => Ok(None),
        Err(e) => Err(format!("{e:?}")),
    }
}

/// Save with the File System Access API. Returns the file name, `Ok(None)` if cancelled.
pub async fn save_native(name: &str, text: &str, save_as: bool) -> Result<Option<String>, String> {
    match fs_save(name, text, save_as).await {
        Ok(v) => Ok(v.as_string()),
        Err(e) if is_abort(&e) => Ok(None),
        Err(e) => Err(format!("{e:?}")),
    }
}

pub async fn read_file(file: &web_sys::File) -> Option<String> {
    file_text(file).await.ok()?.as_string()
}

/// Trigger a browser download of `text`.
pub fn download(name: &str, text: &str, mime: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(a) = doc.create_element("a") {
        let a: web_sys::HtmlAnchorElement = a.unchecked_into();
        a.set_href(&url);
        a.set_download(name);
        a.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn copy_text(text: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
    }
}

pub async fn fetch_text(url: &str) -> Result<String, String> {
    let w = web_sys::window().ok_or("no window")?;
    let resp = JsFuture::from(w.fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp.unchecked_into();
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    text.as_string().ok_or_else(|| "not text".into())
}

pub fn query_param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get(name)
}

/// True when keyboard focus is in a text field (so single-key shortcuts must not fire).
pub fn typing() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .is_some_and(|el| {
            let tag = el.tag_name().to_ascii_lowercase();
            tag == "input"
                || tag == "textarea"
                || tag == "select"
                || el.get_attribute("contenteditable").is_some()
        })
}

pub fn set_root_attr(name: &str, value: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = el.set_attribute(name, value);
    }
}

pub fn scroll_into_view(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    {
        let opts = web_sys::ScrollIntoViewOptions::new();
        opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
        el.scroll_into_view_with_scroll_into_view_options(&opts);
    }
}

pub fn focus(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    {
        let el: web_sys::HtmlElement = el.unchecked_into();
        let _ = el.focus();
    }
}
