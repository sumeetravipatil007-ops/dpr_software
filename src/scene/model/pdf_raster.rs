// PDF underlay rasterisation.
//
// No PDF crate: the page renders through the system `pdftoppm` (poppler-utils
// — present on virtually every Linux desktop, installable on Windows/macOS)
// into a temp PNG, which decodes through the existing `image` dependency.
// When the tool is missing the caller falls back to the underlay's outline
// placeholder, so the feature degrades instead of failing the load.
//
// Pages are cached per (resolved path, page name): underlays re-tessellate on
// every geometry bump, but the expensive external render runs once.

use std::sync::Arc;

/// One rasterised PDF page. `dpi` ties the pixel size back to the page's
/// physical size: `inches = px / dpi`, which is what the underlay's world
/// quad is sized from (1 underlay unit = 1 PDF inch, AutoCAD's convention).
pub struct PdfPage {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub dpi: f32,
}

/// Raster resolution. 150 dpi puts an A4 page at ~1240×1754 px — crisp for
/// normal zooms without ballooning texture memory.
#[cfg(not(target_arch = "wasm32"))]
const RASTER_DPI: u32 = 150;

/// Rasterise `page` (a 1-based page name, e.g. "1") of the PDF at `path`.
/// `None` when the file/page can't be rendered (missing file, no pdftoppm,
/// bad page) — negative results are cached too, so a missing tool doesn't
/// re-spawn a process per tessellation.
#[cfg(not(target_arch = "wasm32"))]
pub fn rasterize_page(path: &str, page: &str) -> Option<Arc<PdfPage>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<(String, String), Option<Arc<PdfPage>>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = (path.to_string(), page.to_string());
    if let Ok(c) = cache.lock() {
        if let Some(hit) = c.get(&key) {
            return hit.clone();
        }
    }
    let built = rasterize_uncached(path, page);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, built.clone());
    }
    built
}

#[cfg(not(target_arch = "wasm32"))]
fn rasterize_uncached(path: &str, page: &str) -> Option<Arc<PdfPage>> {
    use std::process::Command;

    if !std::path::Path::new(path).is_file() {
        return None;
    }
    let page_no: u32 = page.trim().parse().unwrap_or(1).max(1);

    // Unique output prefix in the system temp dir; pdftoppm appends
    // `-<page>` (with version-dependent zero padding) and `.png`.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    (path, page_no).hash(&mut hasher);
    let prefix = std::env::temp_dir().join(format!("ocs_pdf_{:016x}", hasher.finish()));
    let prefix_str = prefix.to_string_lossy().into_owned();

    let status = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            &RASTER_DPI.to_string(),
            "-f",
            &page_no.to_string(),
            "-l",
            &page_no.to_string(),
            path,
            &prefix_str,
        ])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    // Find the produced file: `<prefix>-1.png`, `-01.png`, … depending on the
    // poppler version's padding.
    let dir = prefix.parent()?;
    let stem = prefix.file_name()?.to_string_lossy().into_owned();
    let mut produced: Option<std::path::PathBuf> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&stem) && name.ends_with(".png") {
            produced = Some(entry.path());
            break;
        }
    }
    let png = produced?;
    let decoded = image::open(&png).ok()?.to_rgba8();
    let _ = std::fs::remove_file(&png);
    let (width, height) = decoded.dimensions();
    Some(Arc::new(PdfPage {
        pixels: Arc::new(decoded.into_raw()),
        width,
        height,
        dpi: RASTER_DPI as f32,
    }))
}

/// wasm: no external process — underlays keep their outline placeholder.
#[cfg(target_arch = "wasm32")]
pub fn rasterize_page(_path: &str, _page: &str) -> Option<Arc<PdfPage>> {
    None
}
