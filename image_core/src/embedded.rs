//! Embedded static assets (admin SPA).
//!
//! At compile time we embed the entire contents of `image_core/assets/`.
//! In CI the build script copies `image_webui/out/` into `assets/` before
//! running `cargo build`. In dev / contributor mode `assets/` just contains
//! the placeholder `index.html` shipped in this repo, so the binary still
//! builds and `/` returns a friendly explanation.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Read a file by path from the embedded assets, falling back to
/// `index.html` if the file isn't found (SPA routing).
///
/// Returns `(bytes, mime_type)` on success.
pub fn read(path: &str) -> Option<(Vec<u8>, String)> {
    let normalized = path.trim_start_matches('/');
    if normalized.is_empty() {
        return Assets::get("index.html").map(|f| (f.data.into_owned(), guess_mime("index.html").to_string()));
    }
    // Direct hit — return the actual file with its mime.
    if let Some(f) = Assets::get(normalized) {
        return Some((f.data.into_owned(), guess_mime(normalized).to_string()));
    }
    // SPA fallback — return index.html with text/html mime.
    Assets::get("index.html").map(|f| (f.data.into_owned(), guess_mime("index.html").to_string()))
}

fn guess_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower == "index.html" {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".ttf") {
        "font/ttf"
    } else if lower.ends_with(".map") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_is_embedded() {
        let (bytes, mime) = read("/").expect("index.html must be present");
        assert!(mime.starts_with("text/html"));
        assert!(!bytes.is_empty());
    }

    #[test]
    fn spa_fallback_for_unknown_path() {
        let (bytes, mime) = read("/anything/xyz").expect("SPA fallback");
        assert!(mime.starts_with("text/html"));
        assert!(!bytes.is_empty());
    }

    #[test]
    fn mime_guesses() {
        assert!(guess_mime("foo.js").starts_with("application/javascript"));
        assert!(guess_mime("foo.css").starts_with("text/css"));
        assert!(guess_mime("foo.png").starts_with("image/png"));
        assert!(guess_mime("foo.svg").starts_with("image/svg"));
    }
}