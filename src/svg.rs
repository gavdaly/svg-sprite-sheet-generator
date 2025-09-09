use crate::error::AppError;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winnow::Parser;

mod ids;
mod normalize;
mod parsing;
mod sanitize;
mod transform;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOpts {
    pub quiet: bool,
    pub verbose: bool,
    pub dry_run: bool,
    pub fail_on_warn: bool,
    pub debounce_ms: u64,
    pub poll: bool,
}

// Cached representation of a processed SVG used by incremental watch builds
#[derive(Clone)]
struct CacheEntry {
    mtime: SystemTime,
    len: u64,
    name: String,
    out_attrs: Vec<(String, String)>,
    children: String,
    child_ids: Vec<String>,
    path_str: String,
    warnings: usize,
}

/// A struct to represent a SVG file
#[cfg_attr(not(test), allow(dead_code))]
struct SvgSprite {
    /// The name of the SVG file
    name: String,
    /// The attributes of the svg tag
    attributes: Vec<(String, String)>,
    /// The children of the svg tag
    children: String,
}

impl SvgSprite {
    #[cfg(test)]
    pub fn new(name: String, attributes: Vec<(&str, &str)>, children: String) -> Self {
        let attributes = attributes
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        SvgSprite {
            name,
            attributes,
            children,
        }
    }
}

/// Parse input svgs and stream-write the sprite to the output file to minimize memory usage.
pub fn process_with_opts(directory: &str, file: &str, opts: RunOpts) -> Result<(), AppError> {
    // Collect candidate SVG file entries first to detect empty inputs without creating output.
    let out_basename = std::path::Path::new(file)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    let entries: Vec<std::path::PathBuf> = std::fs::read_dir(directory)
        .map_err(|e| AppError::ReadDir {
            path: directory.to_string(),
            source: e,
        })?
        .filter_map(|e| e.ok().map(|de| de.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "svg"))
        .filter(|p| {
            if let Some(ref base) = out_basename {
                return p.file_name().and_then(|s| s.to_str()) != Some(base.as_str());
            }
            true
        })
        .collect();

    if entries.is_empty() {
        return Err(AppError::NoSvgFiles {
            path: directory.to_string(),
        });
    }

    // Choose output sink based on dry_run
    let writer: Box<dyn std::io::Write> = if opts.dry_run {
        Box::new(std::io::BufWriter::new(std::io::sink()))
    } else {
        let file_opt = std::fs::File::create(file).map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;
        Box::new(std::io::BufWriter::new(file_opt))
    };
    let mut writer = writer;

    use std::io::Write as _;
    writer
        .write_all(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><defs>")
        .map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;

    // Warning counter for this run
    let mut warn_count: usize = 0;

    for path in entries {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let name = file_name.trim_end_matches(".svg").to_string();
        let content = std::fs::read_to_string(&path).map_err(|e| AppError::ReadFile {
            path: path.display().to_string(),
            source: e,
        })?;

        let pre = preprocess_svg_content(&content);
        let mut s = pre.as_str();
        let (attributes, children) = match parsing::parse_svg.parse_next(&mut s) {
            Ok(ok) => ok,
            Err(e) => {
                return Err(AppError::ParseSvg {
                    path: path.display().to_string(),
                    message: format!("{e:?}"),
                });
            }
        };

        // Convert attributes and handle root <svg id> policy: move id -> data-id after sanitization
        let mut out_attrs: Vec<(String, String)> = Vec::new();
        let mut root_id_raw: Option<&str> = None;
        let mut pending_viewbox: Option<String> = None;
        let mut saw_width = false;
        let mut saw_height = false;
        let mut saw_viewbox = false;
        for (k, v) in &attributes {
            if *k == "id" {
                root_id_raw = Some(v);
            } else if *k == "width" || *k == "height" {
                match normalize::normalize_length(v) {
                    Some(nv) => out_attrs.push(((*k).to_string(), nv)),
                    None => {
                        return Err(AppError::InvalidDimension {
                            path: path.display().to_string(),
                            attr: (*k).to_string(),
                            value: (*v).to_string(),
                        });
                    }
                }
                if *k == "width" {
                    saw_width = true;
                } else {
                    saw_height = true;
                }
            } else if *k == "viewBox" {
                match normalize::normalize_viewbox(v) {
                    Some(vb) => pending_viewbox = Some(vb),
                    None => {
                        return Err(AppError::InvalidViewBox {
                            path: path.display().to_string(),
                            value: (*v).to_string(),
                        });
                    }
                }
                saw_viewbox = true;
            } else {
                out_attrs.push(((*k).to_string(), (*v).to_string()));
            }
        }

        if let Some(idv) = root_id_raw {
            let sanitized = sanitize::sanitize_id(idv);
            if sanitized.is_empty() {
                return Err(AppError::InvalidIdAfterSanitize {
                    path: path.display().to_string(),
                    original: idv.to_string(),
                });
            }
            if ids::references_id(children, idv) {
                return Err(AppError::RootIdReferenced {
                    path: path.display().to_string(),
                    id: idv.to_string(),
                });
            }
            // Warning: moved root id to data-id
            warn_count += 1;
            tracing::warn!(path = %path.display(), original_id = %idv, sanitized_id = %sanitized, "Root <svg id> moved to data-id");
            out_attrs.push(("data-id".to_string(), sanitized));
        }
        if let Some(vb) = pending_viewbox {
            out_attrs.push(("viewBox".to_string(), vb));
        }

        // Rewrite internal ids -> data-id and ensure no per-file duplicates
        let (rewritten_children, _data_ids) = ids::rewrite_ids_to_data_ids(children);

        // Warnings for missing dims/viewBox
        if !saw_width {
            warn_count += 1;
            tracing::warn!(path = %path.display(), file = %file_name, "Missing width on root <svg>");
        }
        if !saw_height {
            warn_count += 1;
            tracing::warn!(path = %path.display(), file = %file_name, "Missing height on root <svg>");
        }
        if !saw_viewbox {
            warn_count += 1;
            tracing::warn!(path = %path.display(), file = %file_name, "Missing viewBox on root <svg>");
        }

        let attrs = out_attrs
            .iter()
            .map(|(k, v)| format!(r#" {k}="{v}""#))
            .collect::<String>();
        let pattern = format!(r#"<pattern id="{name}"{attrs}>{rewritten_children}</pattern>"#);
        writer
            .write_all(pattern.as_bytes())
            .map_err(|e| AppError::WriteFile {
                path: file.to_string(),
                source: e,
            })?;
    }

    writer
        .write_all(b"</defs></svg>")
        .and_then(|_| writer.flush())
        .map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;

    if opts.fail_on_warn && warn_count > 0 {
        return Err(AppError::WarningsPresent { count: warn_count });
    }

    Ok(())
}

/// Watch a directory for changes and rebuild the sprite when inputs change.
pub fn watch_with_opts(directory: &str, file: &str, opts: RunOpts) -> Result<(), AppError> {
    if opts.poll {
        return watch_poll(directory, file, opts);
    }
    watch_event(directory, file, opts)
}

pub fn watch_event(directory: &str, file: &str, opts: RunOpts) -> Result<(), AppError> {
    if !opts.quiet {
        tracing::info!(
            directory,
            file,
            mode = "event",
            "Watching for changes (Ctrl+C to stop)"
        );
    }
    use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;

    let (tx, rx) = channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|e| AppError::WriteFile {
        path: "watch".into(),
        source: std::io::Error::other(e.to_string()),
    })?;
    watcher
        .watch(std::path::Path::new(directory), RecursiveMode::NonRecursive)
        .map_err(|e| AppError::ReadDir {
            path: directory.into(),
            source: std::io::Error::other(e.to_string()),
        })?;

    let mut cache: std::collections::HashMap<String, CacheEntry> = std::collections::HashMap::new();
    let debounce = Duration::from_millis(if opts.debounce_ms == 0 {
        1
    } else {
        opts.debounce_ms
    });
    let mut last_trigger = SystemTime::now();
    let mut pending = false;
    loop {
        match rx.recv() {
            Ok(Ok(_evt)) => {
                pending = true;
                let elapsed = last_trigger.elapsed().unwrap_or(Duration::ZERO);
                if elapsed >= debounce {
                    if let Err(e) = rebuild_once(directory, file, &mut cache, opts) {
                        tracing::error!(error = %e, "Rebuild failed");
                    }
                    last_trigger = SystemTime::now();
                    pending = false;
                }
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, "Watch error");
            }
            Err(_disconnected) => break,
        }

        if pending {
            std::thread::sleep(debounce);
            if let Err(e) = rebuild_once(directory, file, &mut cache, opts) {
                tracing::error!(error = %e, "Rebuild failed");
            }
            last_trigger = SystemTime::now();
            pending = false;
        }
    }

    Ok(())
}

pub fn watch_poll(directory: &str, file: &str, opts: RunOpts) -> Result<(), AppError> {
    if !opts.quiet {
        tracing::info!(
            directory,
            file,
            mode = "poll",
            "Watching for changes (Ctrl+C to stop)"
        );
    }
    let mut cache: std::collections::HashMap<String, CacheEntry> = std::collections::HashMap::new();
    let mut last_state: Option<u64> = None;

    loop {
        let state = dir_state_hash(directory)?;
        if last_state.as_ref().is_some_and(|s| *s == state) {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }

        let out_basename = std::path::Path::new(file)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let paths: Vec<std::path::PathBuf> = std::fs::read_dir(directory)
            .map_err(|e| AppError::ReadDir {
                path: directory.to_string(),
                source: e,
            })?
            .filter_map(|e| e.ok().map(|de| de.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "svg"))
            .filter(|p| {
                if let Some(ref base) = out_basename {
                    return p.file_name().and_then(|s| s.to_str()) != Some(base.as_str());
                }
                true
            })
            .collect();

        if paths.is_empty() {
            tracing::warn!(directory, "No SVG files found");
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }

        let live: std::collections::HashSet<_> =
            paths.iter().map(|p| p.display().to_string()).collect();
        cache.retain(|k, _| live.contains(k));

        // Update or add changed files
        for p in &paths {
            let meta = match std::fs::metadata(p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
            let len = meta.len();
            let key = p.display().to_string();
            let needs = match cache.get(&key) {
                Some(c) => c.mtime != mtime || c.len != len,
                None => true,
            };
            if needs {
                match build_cache_entry(p) {
                    Ok(mut ce) => {
                        ce.mtime = mtime;
                        ce.len = len;
                        ce.path_str = key.clone();
                        cache.insert(key, ce);
                    }
                    Err(e) => {
                        eprintln!("Skip {}: {e}", p.display());
                        // skip writing this round
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                }
            }
        }

        // Write sprite from cache in sorted order
        if let Err(e) = write_sprite_from_cache(file, &cache, &paths, opts) {
            eprintln!("Write failed: {e}");
            if let Some(src) = std::error::Error::source(&e) {
                eprintln!("Caused by: {src}");
            }
        } else if !opts.quiet || opts.verbose {
            tracing::info!("Rebuilt sprite");
        }
        last_state = Some(state);
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn dir_state_hash(directory: &str) -> Result<u64, AppError> {
    let entries = std::fs::read_dir(directory).map_err(|e| AppError::ReadDir {
        path: directory.to_string(),
        source: e,
    })?;
    let mut hasher = DefaultHasher::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let _path = entry.path();
        let file_name = entry.file_name();
        let Ok(name_str) = file_name.into_string() else {
            continue;
        };
        if !name_str.ends_with(".svg") {
            continue;
        }
        name_str.hash(&mut hasher);
        if let Ok(md) = entry.metadata() {
            md.len().hash(&mut hasher);
            if let Ok(modified) = md.modified() {
                hash_time(&modified, &mut hasher);
            }
        }
    }
    Ok(hasher.finish())
}

fn hash_time(t: &SystemTime, hasher: &mut DefaultHasher) {
    if let Ok(dur) = t.duration_since(UNIX_EPOCH) {
        dur.as_secs().hash(hasher);
        dur.subsec_nanos().hash(hasher);
    }
}

// Strip BOM, leading XML prolog, and comments before the root <svg> tag
fn preprocess_svg_content(input: &str) -> String {
    let mut s = input.trim_start_matches('\u{feff}');
    // Iteratively skip whitespace + XML declarations or comments before <svg
    loop {
        let trimmed = s.trim_start();
        if trimmed.starts_with("<?") {
            // Skip until '?>'
            if let Some(end) = trimmed.find("?>") {
                s = &trimmed[end + 2..];
                continue;
            }
        } else if trimmed.starts_with("<!--") {
            if let Some(end) = trimmed.find("-->") {
                s = &trimmed[end + 3..];
                continue;
            }
        }
        // If we see neither, stop
        s = trimmed;
        break;
    }
    s.to_string()
}

// Sanitize an id by dropping leading invalid chars and replacing internal
// invalid chars with '-'. Collapse multiple '-' and trim them at ends.
// Allowed pattern: [A-Za-z_][A-Za-z0-9._-]*
// moved: sanitize_id, references_id, extract_ids in submodules

// Parse and normalize positive length values for width/height.
// Accepts unitless or 'px' suffix. Returns normalized string (e.g., "24").
// moved: normalize_* functions in svg::normalize

// sprite rendering moved to svg::transform

fn rebuild_once(
    directory: &str,
    file: &str,
    cache: &mut std::collections::HashMap<String, CacheEntry>,
    opts: RunOpts,
) -> Result<(), AppError> {
    // Collect current svg files
    let paths: Vec<std::path::PathBuf> = std::fs::read_dir(directory)
        .map_err(|e| AppError::ReadDir {
            path: directory.to_string(),
            source: e,
        })?
        .filter_map(|e| e.ok().map(|de| de.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "svg"))
        .collect();

    if paths.is_empty() {
        tracing::warn!(directory, "No SVG files found");
        return Ok(());
    }

    // Remove deleted entries
    let live: std::collections::HashSet<_> =
        paths.iter().map(|p| p.display().to_string()).collect();
    cache.retain(|k, _| live.contains(k));

    // Update or add changed files
    for p in &paths {
        let meta = match std::fs::metadata(p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        let len = meta.len();
        let key = p.display().to_string();
        let needs = match cache.get(&key) {
            Some(c) => c.mtime != mtime || c.len != len,
            None => true,
        };
        if needs {
            match build_cache_entry(p) {
                Ok(mut ce) => {
                    ce.mtime = mtime;
                    ce.len = len;
                    ce.path_str = key.clone();
                    cache.insert(key, ce);
                }
                Err(e) => {
                    tracing::warn!(path = %p.display(), error = %e, "Skipping file due to error");
                    return Ok(());
                }
            }
        }
    }

    // Check global id collisions
    let mut id_reg: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for entry in cache.values() {
        for cid in &entry.child_ids {
            if let Some(first) = id_reg.get(cid) {
                return Err(AppError::IdCollision {
                    id: cid.clone(),
                    first_path: first.clone(),
                    second_path: entry.path_str.clone(),
                });
            } else {
                id_reg.insert(cid.clone(), entry.path_str.clone());
            }
        }
    }

    write_sprite_from_cache(file, cache, &paths, opts)
}

fn build_cache_entry(path: &std::path::Path) -> Result<CacheEntry, AppError> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::ReadFile {
            path: path.display().to_string(),
            source: std::io::Error::other("invalid filename"),
        })?
        .to_string();
    let name = file_name.trim_end_matches(".svg").to_string();
    let content = std::fs::read_to_string(path).map_err(|e| AppError::ReadFile {
        path: path.display().to_string(),
        source: e,
    })?;
    let pre = preprocess_svg_content(&content);
    let mut s = pre.as_str();
    let (attributes, children) =
        parsing::parse_svg
            .parse_next(&mut s)
            .map_err(|e| AppError::ParseSvg {
                path: path.display().to_string(),
                message: format!("{e:?}"),
            })?;

    let mut out_attrs: Vec<(String, String)> = Vec::new();
    let mut root_id_raw: Option<&str> = None;
    let mut pending_viewbox: Option<String> = None;
    let mut warnings = 0usize;
    let mut saw_width = false;
    let mut saw_height = false;
    let mut saw_viewbox = false;
    for (k, v) in &attributes {
        if *k == "id" {
            root_id_raw = Some(v);
        } else if *k == "width" || *k == "height" {
            match normalize::normalize_length(v) {
                Some(nv) => out_attrs.push(((*k).to_string(), nv)),
                None => {
                    return Err(AppError::InvalidDimension {
                        path: path.display().to_string(),
                        attr: (*k).to_string(),
                        value: (*v).to_string(),
                    });
                }
            }
            if *k == "width" {
                saw_width = true;
            } else {
                saw_height = true;
            }
        } else if *k == "viewBox" {
            match normalize::normalize_viewbox(v) {
                Some(vb) => pending_viewbox = Some(vb),
                None => {
                    return Err(AppError::InvalidViewBox {
                        path: path.display().to_string(),
                        value: (*v).to_string(),
                    });
                }
            }
            saw_viewbox = true;
        } else {
            out_attrs.push(((*k).to_string(), (*v).to_string()));
        }
    }
    if let Some(idv) = root_id_raw {
        let sanitized = sanitize::sanitize_id(idv);
        if sanitized.is_empty() {
            return Err(AppError::InvalidIdAfterSanitize {
                path: path.display().to_string(),
                original: idv.to_string(),
            });
        }
        if ids::references_id(children, idv) {
            return Err(AppError::RootIdReferenced {
                path: path.display().to_string(),
                id: idv.to_string(),
            });
        }
        warnings += 1;
        out_attrs.push(("data-id".to_string(), sanitized));
    }
    if let Some(vb) = pending_viewbox {
        out_attrs.push(("viewBox".to_string(), vb));
    }

    let (rewritten_children, data_ids) = ids::rewrite_ids_to_data_ids(children);

    if !saw_width {
        warnings += 1;
    }
    if !saw_height {
        warnings += 1;
    }
    if !saw_viewbox {
        warnings += 1;
    }

    Ok(CacheEntry {
        mtime: UNIX_EPOCH,
        len: 0,
        name,
        out_attrs,
        children: rewritten_children,
        child_ids: data_ids,
        path_str: path.display().to_string(),
        warnings,
    })
}

fn write_sprite_from_cache(
    file: &str,
    cache: &std::collections::HashMap<String, CacheEntry>,
    order: &[std::path::PathBuf],
    opts: RunOpts,
) -> Result<(), AppError> {
    use std::io::Write as _;
    // Use dry-run sink when requested
    let writer: Box<dyn std::io::Write> = if opts.dry_run {
        Box::new(std::io::BufWriter::new(std::io::sink()))
    } else {
        let file_opt = std::fs::File::create(file).map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;
        Box::new(std::io::BufWriter::new(file_opt))
    };
    let mut writer = writer;
    writer
        .write_all(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><defs>")
        .map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;

    let mut warn_count = 0usize;
    for p in order {
        let key = p.display().to_string();
        if let Some(entry) = cache.get(&key) {
            warn_count += entry.warnings;
            let attrs = entry
                .out_attrs
                .iter()
                .map(|(k, v)| format!(r#" {k}="{v}""#))
                .collect::<String>();
            writer
                .write_all(
                    format!(
                        r#"<pattern id="{}"{}>{}</pattern>"#,
                        entry.name, attrs, entry.children
                    )
                    .as_bytes(),
                )
                .map_err(|e| AppError::WriteFile {
                    path: file.to_string(),
                    source: e,
                })?;
        }
    }

    writer
        .write_all(b"</defs></svg>")
        .and_then(|_| writer.flush())
        .map_err(|e| AppError::WriteFile {
            path: file.to_string(),
            source: e,
        })?;

    if opts.fail_on_warn && warn_count > 0 {
        return Err(AppError::WarningsPresent { count: warn_count });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::{fs, path::PathBuf};

    // Simple temp dir guard to keep tests isolated
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut p = std::env::temp_dir();
            let unique = format!("{}_{}", prefix, std::process::id());
            p.push(unique);
            // Best-effort cleanup if it already exists
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn transform_emits_pattern_per_file() {
        let svgs = vec![
            SvgSprite::new(
                "one".into(),
                vec![("width", "24"), ("height", "24")],
                "<g/>".into(),
            ),
            SvgSprite::new("two".into(), vec![("fill", "#000")], "<circle/>".into()),
        ];
        let out = crate::svg::transform::transform(svgs);
        assert!(out.starts_with("<svg"));
        assert!(out.contains("<defs>"));
        assert!(out.contains("<pattern id=\"one\" width=\"24\" height=\"24\"><g/>"));
        assert!(out.contains("<pattern id=\"two\" fill=\"#000\"><circle/>"));
        assert!(out.ends_with("</defs></svg>"));
    }

    #[test]
    fn process_empty_directory_yields_error() {
        let tmp = TempDir::new("svg_sheet_empty");
        let err = process_with_opts(
            tmp.path().to_str().unwrap(),
            &tmp.path().join("out.svg").display().to_string(),
            RunOpts::default(),
        )
        .expect_err("expected error");
        match err {
            AppError::NoSvgFiles { .. } => {}
            _ => panic!("wrong error: {err}"),
        }
    }

    #[test]
    fn load_and_build_from_real_files() {
        let tmp = TempDir::new("svg_sheet_build");
        let dir = tmp.path();
        // Create a couple of minimal SVGs
        fs::write(
            dir.join("a.svg"),
            "<svg width=\"10\" height=\"10\"><rect/></svg>",
        )
        .unwrap();
        fs::write(dir.join("b.svg"), "<svg id=\"b\"><g/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("build ok");
        let sprite = fs::read_to_string(&out).expect("read sprite");
        assert!(sprite.contains("pattern id=\"a\""));
        assert!(sprite.contains("pattern id=\"b\""));
    }

    #[test]
    fn dir_state_hash_changes_on_update() {
        let tmp = TempDir::new("svg_sheet_hash");
        let dir = tmp.path();
        fs::write(dir.join("c.svg"), "<svg id=\"c\"></svg>").unwrap();
        let h1 = dir_state_hash(dir.to_str().unwrap()).expect("hash1");
        // Touch file update
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir.join("c.svg"), "<svg id=\"c2\"></svg>").unwrap();
        let h2 = dir_state_hash(dir.to_str().unwrap()).expect("hash2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn sanitize_id_drops_leading_and_replaces_invalids() {
        assert_eq!(crate::svg::sanitize::sanitize_id("123abc"), "abc");
        assert_eq!(crate::svg::sanitize::sanitize_id("-foo"), "foo");
        assert_eq!(crate::svg::sanitize::sanitize_id("💥x"), "x");
        assert_eq!(
            crate::svg::sanitize::sanitize_id("data icon@1.5x"),
            "data-icon-1.5x"
        );
    }

    #[test]
    fn root_svg_id_is_moved_to_data_id_and_sanitized() {
        let tmp = TempDir::new("root_id_move");
        let dir = tmp.path();
        fs::write(
            dir.join("logo.svg"),
            "<svg id=\"123Logo\" width=\"1\" height=\"1\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("build ok");
        let sprite = fs::read_to_string(&out).expect("read sprite");
        // id removed, data-id present with sanitized value "Logo"
        assert!(sprite.contains("data-id=\"Logo\""));
        assert!(!sprite.contains(" id=\"123Logo\""));
    }

    #[test]
    fn root_svg_id_reference_causes_error() {
        let tmp = TempDir::new("root_id_ref");
        let dir = tmp.path();
        fs::write(
            dir.join("r.svg"),
            "<svg id=\"root\"><use href=\"#root\"/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        let err = process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect_err("should err");
        match err {
            AppError::RootIdReferenced { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn ids_rewritten_to_data_ids_and_no_cross_file_collision() {
        let tmp = TempDir::new("id_rewrite");
        let dir = tmp.path();
        fs::write(dir.join("a.svg"), "<svg width='1'><g id=\"dup\"/></svg>").unwrap();
        fs::write(dir.join("b.svg"), "<svg width='1'><g id=\"dup\"/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("should build ok, no cross-file id collision");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("data-id=\"dup\""));
        assert!(!sprite.contains(" id=\"dup\""));
    }

    #[test]
    fn handles_bom_xml_prolog_and_leading_comment() {
        let tmp = TempDir::new("svg_preamble");
        let dir = tmp.path();
        let content = format!(
            "{}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- comment -->\n<svg width=\"10\" height=\"10\"><rect/></svg>",
            '\u{feff}'
        );
        fs::write(dir.join("p.svg"), content).unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("pattern id=\"p\""));
    }

    #[test]
    fn normalizes_width_height_values() {
        let tmp = TempDir::new("svg_dims_norm");
        let dir = tmp.path();
        fs::write(
            dir.join("s.svg"),
            "<svg width=\"24px\" height=\"24.0\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("width=\"24\""));
        assert!(sprite.contains("height=\"24\""));
    }

    #[test]
    fn rejects_invalid_dimension_values() {
        let tmp = TempDir::new("svg_dims_reject");
        let dir = tmp.path();
        fs::write(dir.join("s.svg"), "<svg width=\"0\" height=\"1\"></svg>").unwrap();
        let out = dir.join("sprite.svg");
        let err = process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect_err("should err");
        match err {
            AppError::InvalidDimension { attr, .. } => assert_eq!(attr, "width"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn normalizes_viewbox() {
        let tmp = TempDir::new("svg_viewbox_norm");
        let dir = tmp.path();
        fs::write(
            dir.join("v.svg"),
            "<svg viewBox=\"0,0,24,24\" width=\"1\"><g/></svg>",
        )
        .unwrap();
        let out = dir.join("sprite.svg");
        process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect("build ok");
        let sprite = fs::read_to_string(&out).unwrap();
        assert!(sprite.contains("viewBox=\"0 0 24 24\""));
    }

    #[test]
    fn rejects_invalid_viewbox_dims() {
        let tmp = TempDir::new("svg_viewbox_reject");
        let dir = tmp.path();
        fs::write(dir.join("v.svg"), "<svg viewBox=\"0 0 0 24\"><g/></svg>").unwrap();
        let out = dir.join("sprite.svg");
        let err = process_with_opts(
            dir.to_str().unwrap(),
            out.to_str().unwrap(),
            RunOpts::default(),
        )
        .expect_err("should err");
        match err {
            AppError::InvalidViewBox { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    // Property tests for sanitize_id live in svg::sanitize

    // Normalization property tests live in svg::normalize

    // ID generators moved to svg::ids tests

    // Property tests for ids::extract_ids live in svg::ids

    // Property: preprocess_svg_content removes BOM, xml prolog, and leading comments before <svg>
    proptest! {
        #[test]
        fn prop_preprocess_svg_preamble_stripped(
            n_comments in 0usize..3,
            include_bom in proptest::bool::ANY,
            include_prolog in proptest::bool::ANY
        ) {
            let mut s = String::new();
            if include_bom { s.push('\u{feff}'); }
            if include_prolog { s.push_str("<?xml version=\"1.0\"?>"); }
            for i in 0..n_comments { s.push_str(&format!("<!-- c{i} -->")); }
            s.push_str("<svg width=\"1\"></svg>");
            let pre = preprocess_svg_content(&s);
            prop_assert!(pre.starts_with("<svg"));
        }
    }

    // Property tests for ids::references_id live in svg::ids
}
