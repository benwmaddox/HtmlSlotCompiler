use chrono::Local;
use kuchiki::traits::*;
use kuchiki::{parse_html, NodeRef};
use notify::{RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotClosingStyle {
    Explicit,
    SelfClosing,
    Void,
}

#[derive(Debug, Clone)]
struct SlotSpec {
    name: String,
    mode: String,
    layout_tag: String,
    closing_style: SlotClosingStyle,
}

#[derive(Debug, Clone)]
struct PageSlotContent {
    tag: String,
    inner_html: String,
    attributes: HashMap<String, String>,
    original_html: Option<String>,
    closing_style: SlotClosingStyle,
}

const VOID_TAGS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

struct Compiler {
    src_dir: PathBuf,
    out_dir: PathBuf,
    layout_path: PathBuf,
}

impl PageSlotContent {
    fn render(&self) -> String {
        if let Some(original) = &self.original_html {
            original.clone()
        } else {
            Self::build_markup(
                &self.tag,
                &self.attributes,
                &self.inner_html,
                self.closing_style,
            )
        }
    }

    fn build_markup(
        tag: &str,
        attributes: &HashMap<String, String>,
        inner_html: &str,
        closing_style: SlotClosingStyle,
    ) -> String {
        let mut attrs: Vec<(&String, &String)> = attributes.iter().collect();
        attrs.sort_by(|a, b| match (a.0.as_str(), b.0.as_str()) {
            ("for-slot", "for-slot") => Ordering::Equal,
            ("for-slot", _) => Ordering::Less,
            (_, "for-slot") => Ordering::Greater,
            _ => a.0.cmp(b.0),
        });

        let mut attr_string = String::new();
        for (key, value) in attrs {
            attr_string.push(' ');
            attr_string.push_str(key);
            attr_string.push_str("=\"");
            attr_string.push_str(&value.replace('"', "&quot;"));
            attr_string.push('"');
        }

        match closing_style {
            SlotClosingStyle::SelfClosing => format!("<{}{} />", tag, attr_string),
            SlotClosingStyle::Void => format!("<{}{}>", tag, attr_string),
            SlotClosingStyle::Explicit => {
                format!("<{}{}>{}</{}>", tag, attr_string, inner_html, tag)
            }
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    VOID_TAGS.contains(&lower.as_str())
}

fn determine_closing_style(layout_html: &str, tag: &str, slot_name: &str) -> SlotClosingStyle {
    let pattern = format!(
        r#"(?is)<{tag}\b[^>]*\bslot\s*=\s*["']{slot}["'][^>]*>"#,
        tag = regex::escape(tag),
        slot = regex::escape(slot_name)
    );

    if let Ok(re) = regex::Regex::new(&pattern) {
        if let Some(mat) = re.find(layout_html) {
            let snippet = mat.as_str().trim_end();
            if snippet.ends_with("/>") {
                return SlotClosingStyle::SelfClosing;
            }
            if is_void_element(tag) {
                return SlotClosingStyle::Void;
            }
            return SlotClosingStyle::Explicit;
        }
    }

    if is_void_element(tag) {
        SlotClosingStyle::Void
    } else {
        SlotClosingStyle::Explicit
    }
}

fn strip_attribute(fragment: &str, attr: &str) -> String {
    let pattern = format!(
        r#"(?i)\s+{attr}\s*=\s*(?:"[^"]*"|'[^']*')"#,
        attr = regex::escape(attr)
    );

    if let Ok(re) = regex::Regex::new(&pattern) {
        re.replace_all(fragment, "").to_string()
    } else {
        fragment.to_string()
    }
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }

    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a_canon), Ok(b_canon)) => a_canon == b_canon,
        _ => false,
    }
}

fn write_if_changed(path: &Path, contents: &str) -> std::io::Result<bool> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == contents {
            return Ok(false);
        }
    }

    fs::write(path, contents)?;
    Ok(true)
}

fn format_with_commas(value: u128) -> String {
    let digits: Vec<char> = value.to_string().chars().collect();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);

    for (idx, ch) in digits.iter().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(*ch);
    }

    formatted.chars().rev().collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let src_dir_arg = args.get(1).map(|s| s.as_str()).unwrap_or("src");
    let out_dir_arg = args.get(2).map(|s| s.as_str()).unwrap_or("dist");
    let watch = args.get(3).map(|s| s.as_str()) == Some("--watch");

    let src_dir_path = Path::new(src_dir_arg);
    if !src_dir_path.exists() {
        eprintln!("[Error] Source directory not found: {}", src_dir_arg);
        std::process::exit(1);
    }

    let raw_layout_path = src_dir_path.join("_layout.html");
    if !raw_layout_path.exists() {
        eprintln!("[Error] Missing {}", raw_layout_path.display());
        std::process::exit(1);
    }

    let src_dir = src_dir_path
        .canonicalize()
        .unwrap_or_else(|_| src_dir_path.to_path_buf());
    let out_dir = Path::new(out_dir_arg).to_path_buf();
    let layout_path = src_dir.join("_layout.html");

    let compiler = Compiler {
        src_dir: src_dir.clone(),
        out_dir: out_dir.clone(),
        layout_path,
    };
    compiler.clean_output_dir();

    let ok = compiler.build_once(None);
    if !watch {
        if !ok {
            std::process::exit(2);
        }
        return;
    }

    println!("[Watch] Watching for changes…");

    let src_dir_clone = compiler.src_dir.clone();
    let out_dir_clone = compiler.out_dir.clone();
    let layout_path_clone = compiler.layout_path.clone();

    let pending = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
    let pending_clone = Arc::clone(&pending);

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                for path in event.paths {
                    let is_tmp = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("tmp"))
                        .unwrap_or(false);
                    if is_tmp {
                        continue;
                    }
                    let _ = tx.send(path);
                }
            }
            Err(_) => {}
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[Error] {}", e);
                return;
            }
        };

    let _ = watcher.watch(&src_dir_clone, RecursiveMode::Recursive);

    let mut timer_active = false;
    let mut last_build = std::time::Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_millis(150)) {
            Ok(path) => {
                let normalized = path.canonicalize().unwrap_or(path.clone());
                pending_clone.lock().unwrap().insert(normalized);
                timer_active = true;
                last_build = std::time::Instant::now();
            }
            Err(_) => {
                if timer_active && last_build.elapsed() >= Duration::from_millis(150) {
                    let changed_paths = {
                        let mut guard = pending_clone.lock().unwrap();
                        guard.drain().collect::<HashSet<PathBuf>>()
                    };
                    let compiler = Compiler {
                        src_dir: src_dir_clone.clone(),
                        out_dir: out_dir_clone.clone(),
                        layout_path: layout_path_clone.clone(),
                    };
                    compiler.build_once(Some(&changed_paths));
                    timer_active = false;
                }
            }
        }
    }
}

impl Compiler {
    fn build_once(&self, changed_paths: Option<&HashSet<PathBuf>>) -> bool {
        let start = Instant::now();
        let now = Local::now();
        println!("[Build] {}", now.format("%H:%M:%S"));

        let _ = fs::create_dir_all(&self.out_dir);

        let layout_html = match fs::read_to_string(&self.layout_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("[Error] {}", e);
                return false;
            }
        };

        // Parse layout and extract slots
        let layout_doc = parse_html().one(layout_html.clone());

        let mut slots = Vec::new();
        for element in layout_doc.select("[slot]").unwrap() {
            let node = element.as_node();
            let attrs = node.as_element().unwrap().attributes.borrow();

            let name = attrs.get("slot").unwrap_or("").to_string();
            let mode = attrs.get("slot-mode").unwrap_or("html").to_string();
            let layout_tag = node.as_element().unwrap().name.local.to_string();
            let closing_style = determine_closing_style(&layout_html, &layout_tag, &name);

            slots.push(SlotSpec {
                name,
                mode,
                layout_tag,
                closing_style,
            });
        }

        if slots.is_empty() {
            println!("[Warn] No slots in _layout.html. Nothing to merge.");
        }

        let mut overall_ok = true;
        let layout_names: HashSet<String> = slots.iter().map(|s| s.name.clone()).collect();

        let src_dir_canonical = self
            .src_dir
            .canonicalize()
            .unwrap_or_else(|_| self.src_dir.clone());
        let layout_aliases = self.build_layout_aliases();

        let mut full_rebuild = changed_paths.is_none();
        if let Some(paths) = changed_paths {
            if paths.is_empty() {
                full_rebuild = true;
            } else if paths.iter().any(|path| self.path_missing_with_retry(path)) {
                full_rebuild = true;
            } else if paths.iter().any(|path| {
                self.path_matches_layout(path, &layout_aliases, src_dir_canonical.as_path())
            }) {
                full_rebuild = true;
            }
        }

        if !full_rebuild {
            if let Ok(entries) = fs::read_dir(&self.src_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !self.is_html_file(&path) {
                        continue;
                    }
                    let file_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name,
                        None => continue,
                    };
                    if file_name.eq_ignore_ascii_case("_layout.html") {
                        continue;
                    }
                    if !self.out_dir.join(file_name).exists() {
                        full_rebuild = true;
                        break;
                    }
                }
            }
        }

        if let Some(paths) = changed_paths {
            for path in paths {
                if self.path_missing_with_retry(path) {
                    self.remove_output_for_path(path);
                }
            }
        }

        let mut page_paths: Vec<PathBuf> = Vec::new();

        if full_rebuild {
            let entries = match fs::read_dir(&self.src_dir) {
                Ok(entries) => entries,
                Err(e) => {
                    eprintln!("[Error] {}", e);
                    return false;
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(page_path) =
                    self.normalize_watch_path(&path, src_dir_canonical.as_path(), &layout_aliases)
                {
                    page_paths.push(page_path);
                }
            }
        } else if let Some(paths) = changed_paths {
            let mut seen = HashSet::new();
            for path in paths {
                if let Some(page_path) =
                    self.normalize_watch_path(path, src_dir_canonical.as_path(), &layout_aliases)
                {
                    if seen.insert(page_path.clone()) {
                        page_paths.push(page_path);
                    }
                }
            }
        }

        for path in page_paths {
            let file_name = match path.file_name().and_then(|name| name.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if !path.exists() {
                continue;
            }

            let page_html = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("[Error] {}", e);
                    overall_ok = false;
                    continue;
                }
            };

            let page_doc = parse_html().one(page_html.clone());

            // Extract page slots with metadata for normalization
            let mut page_slots: HashMap<String, PageSlotContent> = HashMap::new();
            let mut page_slot_order: Vec<String> = Vec::new();

            for element in page_doc.select("[for-slot]").unwrap() {
                let node = element.as_node();
                let attrs_ref = node.as_element().unwrap().attributes.borrow();

                if let Some(slot_name) = attrs_ref.get("for-slot") {
                    if !page_slots.contains_key(slot_name) {
                        let slot_name_string = slot_name.to_string();
                        let tag_name = node.as_element().unwrap().name.local.to_string();

                        let mut attributes = HashMap::new();
                        for (attr_name, attr_value) in attrs_ref.map.iter() {
                            attributes
                                .insert(attr_name.local.to_string(), attr_value.value.clone());
                        }

                        let outer_html = self.get_outer_html(node);
                        let inner_html = self.get_inner_html(node);

                        page_slot_order.push(slot_name_string.clone());
                        let trimmed_outer = outer_html.trim_end();
                        let lower_outer = trimmed_outer.to_ascii_lowercase();
                        let closing_probe = format!("</{}>", tag_name.to_ascii_lowercase());

                        let closing_style = if trimmed_outer.ends_with("/>") {
                            SlotClosingStyle::SelfClosing
                        } else if lower_outer.contains(&closing_probe) {
                            SlotClosingStyle::Explicit
                        } else if is_void_element(&tag_name) {
                            SlotClosingStyle::Void
                        } else {
                            SlotClosingStyle::Explicit
                        };

                        page_slots.insert(
                            slot_name_string,
                            PageSlotContent {
                                tag: tag_name,
                                inner_html,
                                attributes,
                                original_html: if outer_html.is_empty() {
                                    None
                                } else {
                                    Some(outer_html)
                                },
                                closing_style,
                            },
                        );
                    }
                }
            }

            // Check for unknown slots
            let mut extra = Vec::new();
            for slot_name in page_slots.keys() {
                if !layout_names.contains(slot_name) {
                    extra.push(slot_name.clone());
                }
            }

            if !extra.is_empty() {
                println!(
                    "[Error] {} has unknown slots: {}",
                    file_name,
                    extra.join(", ")
                );
                overall_ok = false;
                continue;
            }

            let expected_order: Vec<String> = slots
                .iter()
                .filter(|slot| page_slots.contains_key(&slot.name))
                .map(|slot| slot.name.clone())
                .collect();
            let order_changed = page_slot_order != expected_order;

            let mut page_slots_for_merge = page_slots.clone();
            let mut missing_slots = Vec::new();
            for slot in &slots {
                if !page_slots_for_merge.contains_key(&slot.name) {
                    missing_slots.push(slot.name.clone());
                    page_slots_for_merge
                        .insert(slot.name.clone(), self.default_slot_provider(slot));
                }
            }

            if !missing_slots.is_empty() {
                println!(
                    "[Normalize] Added missing slots in {}: {}",
                    file_name,
                    missing_slots.join(", ")
                );
            }

            if order_changed {
                println!(
                    "[Normalize] Reordered slots to match layout for {}",
                    file_name
                );
            }

            let uses_crlf = page_html.contains("\r\n");
            let had_trailing_newline = page_html.ends_with('\n') || page_html.ends_with("\r\n");

            let mut normalized_blocks = Vec::new();
            for slot in &slots {
                if let Some(content) = page_slots_for_merge.get(&slot.name) {
                    normalized_blocks.push(content.render());
                }
            }

            let normalized_join = normalized_blocks.join("\n\n");
            let normalized_compare = normalized_join.trim_end_matches('\n').to_string();

            let original_compare = page_html
                .replace("\r\n", "\n")
                .trim_end_matches('\n')
                .to_string();

            if normalized_compare != original_compare {
                let mut final_text = normalized_compare.clone();
                if had_trailing_newline {
                    final_text.push('\n');
                }
                if uses_crlf {
                    final_text = final_text.replace("\n", "\r\n");
                }

                match write_if_changed(&path, &final_text) {
                    Ok(true) => {
                        println!("[Normalize] Wrote {}", file_name);
                    }
                    Ok(false) => {
                        // Already up to date; nothing to do.
                    }
                    Err(e) => {
                        eprintln!("[Error] {}", e);
                        overall_ok = false;
                        continue;
                    }
                }
            }

            // Build output by merging page slots into layout (string-based to preserve whitespace)
            let mut output_html = layout_html.clone();

            for slot in &slots {
                if let Some(content) = page_slots_for_merge.get(&slot.name) {
                    output_html = self.merge_slot_string(&output_html, slot, content);
                }
            }

            let dest_path = self.out_dir.join(&file_name);
            let _ = fs::create_dir_all(dest_path.parent().unwrap());
            match write_if_changed(&dest_path, &output_html) {
                Ok(true) => println!("✔  Built {}", file_name),
                Ok(false) => println!("- Built {} (unchanged)", file_name),
                Err(e) => {
                    eprintln!("[Error] {}", e);
                    overall_ok = false;
                    continue;
                }
            }
        }

        self.copy_assets_diff();
        let elapsed_ms = start.elapsed().as_millis();
        println!(
            "[Build] Complete in {} ms.\n",
            format_with_commas(elapsed_ms)
        );

        overall_ok
    }

    fn build_layout_aliases(&self) -> Vec<PathBuf> {
        let mut aliases = vec![self.layout_path.clone()];
        if let Ok(canonical) = self.layout_path.canonicalize() {
            if !aliases.iter().any(|alias| *alias == canonical) {
                aliases.push(canonical);
            }
        }
        aliases
    }

    fn path_matches_layout(
        &self,
        path: &Path,
        layout_aliases: &[PathBuf],
        src_dir_canonical: &Path,
    ) -> bool {
        if layout_aliases
            .iter()
            .any(|alias| paths_equivalent(alias.as_path(), path))
        {
            return true;
        }

        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.eq_ignore_ascii_case("_layout.html") {
                if let Some(parent) = path.parent() {
                    if paths_equivalent(parent, src_dir_canonical)
                        || paths_equivalent(parent, self.src_dir.as_path())
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn normalize_watch_path(
        &self,
        path: &Path,
        src_dir_canonical: &Path,
        layout_aliases: &[PathBuf],
    ) -> Option<PathBuf> {
        let mut candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.src_dir.join(path)
        };

        if let Ok(canonical) = candidate.canonicalize() {
            candidate = canonical;
        }

        if !candidate.exists() {
            return None;
        }

        if !candidate.starts_with(src_dir_canonical) {
            return None;
        }

        if self.path_matches_layout(&candidate, layout_aliases, src_dir_canonical) {
            return None;
        }

        if !self.is_html_file(&candidate) {
            return None;
        }

        Some(candidate)
    }

    fn is_html_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("html"))
            .unwrap_or(false)
    }

    fn path_missing_with_retry(&self, path: &Path) -> bool {
        if path.exists() {
            return false;
        }
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(10));
            if path.exists() {
                return false;
            }
        }
        true
    }

    fn get_inner_html(&self, node: &NodeRef) -> String {
        // Get the inner HTML by serializing all children
        let mut result = Vec::new();
        for child in node.children() {
            let mut child_html = Vec::new();
            child.serialize(&mut child_html).ok();
            result.extend(child_html);
        }
        String::from_utf8_lossy(&result).to_string()
    }

    fn get_outer_html(&self, node: &NodeRef) -> String {
        let mut result = Vec::new();
        node.serialize(&mut result).ok();
        String::from_utf8_lossy(&result).to_string()
    }

    fn default_slot_provider(&self, slot: &SlotSpec) -> PageSlotContent {
        let mut attributes: HashMap<String, String> = HashMap::new();
        attributes.insert("for-slot".to_string(), slot.name.clone());

        if let Some(attr_name) = slot.mode.strip_prefix("attr:") {
            attributes.insert(attr_name.to_string(), String::new());
        }

        // Keep defaults blank so normalized pages clearly signal fields to fill in.
        PageSlotContent {
            tag: slot.layout_tag.clone(),
            inner_html: String::new(),
            attributes,
            original_html: None,
            closing_style: slot.closing_style,
        }
    }

    fn merge_slot_string(&self, html: &str, slot: &SlotSpec, content: &PageSlotContent) -> String {
        if matches!(
            slot.closing_style,
            SlotClosingStyle::SelfClosing | SlotClosingStyle::Void
        ) {
            let pattern = format!(
                r#"(?is)(<{tag}\b[^>]*\bslot\s*=\s*["']{name}["'][^>]*)(\s*/?>)"#,
                tag = regex::escape(&slot.layout_tag),
                name = regex::escape(&slot.name)
            );

            let re = regex::Regex::new(&pattern).unwrap();

            return re
                .replace(html, |caps: &regex::Captures| {
                    let ending = &caps[2];
                    let without_slot = strip_attribute(&caps[1], "slot");
                    let without_mode = strip_attribute(&without_slot, "slot-mode");
                    let opening_tag = without_mode.trim_end().to_string();

                    match slot.mode.as_str() {
                        mode if mode.starts_with("attr:") => {
                            let attr_name = &mode[5..];
                            if let Some(value) = content.attributes.get(attr_name) {
                                let mut builder = opening_tag;
                                if !builder.ends_with(' ') {
                                    builder.push(' ');
                                }
                                builder.push_str(attr_name);
                                builder.push_str("=\"");
                                builder.push_str(value);
                                builder.push('"');

                                format!("{}{}", builder, ending)
                            } else {
                                format!("{}{}", opening_tag, ending)
                            }
                        }
                        _ => format!("{}{}", opening_tag, ending),
                    }
                })
                .to_string();
        }

        // Build the search pattern for the slot element
        // Match: <tag ...slot="name"...>...</tag>
        let pattern = format!(
            r#"(?is)(<{tag}\b[^>]*\bslot\s*=\s*["']{name}["'][^>]*>)(.*?)(</{tag}>)"#,
            tag = regex::escape(&slot.layout_tag),
            name = regex::escape(&slot.name)
        );

        let re = regex::Regex::new(&pattern).unwrap();

        re.replace(html, |caps: &regex::Captures| {
            let opening_tag = strip_attribute(&caps[1], "slot");
            let opening_tag = strip_attribute(&opening_tag, "slot-mode");
            let opening_tag = opening_tag.trim_end().to_string();
            let closing_tag = &caps[3];

            match slot.mode.as_str() {
                "text" => {
                    // For text mode, insert content as plain text
                    format!("{}{}{}", opening_tag, &content.inner_html, closing_tag)
                }
                mode if mode.starts_with("attr:") => {
                    // For attr mode, copy attribute value from the provider element
                    let attr_name = &mode[5..];

                    if let Some(value) = content.attributes.get(attr_name) {
                        let tag_with_attr =
                            opening_tag.replace(">", &format!(r#" {}="{}">"#, attr_name, value));
                        format!("{}{}", tag_with_attr, closing_tag)
                    } else {
                        format!("{}{}", opening_tag, closing_tag)
                    }
                }
                _ => {
                    // For html mode (default), insert content as HTML
                    format!("{}{}{}", opening_tag, &content.inner_html, closing_tag)
                }
            }
        })
        .to_string()
    }

    fn copy_assets_diff(&self) {
        for entry in WalkDir::new(&self.src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_string_lossy();

            if file_name.ends_with(".html") {
                continue;
            }

            let rel_path = path.strip_prefix(&self.src_dir).unwrap();
            let dest = self.out_dir.join(rel_path);

            let _ = fs::create_dir_all(dest.parent().unwrap());

            let needs_copy = if dest.exists() {
                !self.file_hash_equal(path, &dest)
            } else {
                true
            };

            if needs_copy {
                if let Err(e) = fs::copy(path, &dest) {
                    eprintln!("[Error] {}", e);
                } else {
                    println!("📁 Copied {}", rel_path.display());
                }
            }
        }
    }

    fn clean_output_dir(&self) {
        let expected = self.expected_output_set();

        if !self.out_dir.exists() {
            let _ = fs::create_dir_all(&self.out_dir);
            return;
        }

        let mut files_to_remove = Vec::new();
        for entry in WalkDir::new(&self.out_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let rel = match path.strip_prefix(&self.out_dir) {
                Ok(p) => p.to_path_buf(),
                Err(_) => continue,
            };

            if path.is_file() && !expected.contains(&rel) {
                files_to_remove.push(path.to_path_buf());
            }
        }

        for file in files_to_remove {
            let rel = file.strip_prefix(&self.out_dir).unwrap_or(file.as_path());
            if let Err(e) = fs::remove_file(&file) {
                eprintln!("[Warn] Failed to remove {}: {}", rel.display(), e);
            } else {
                println!("[Cleanup] Removed {}", rel.display());
            }
        }

        // Remove empty directories deepest first
        let mut dirs: Vec<PathBuf> = WalkDir::new(&self.out_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path().to_path_buf())
            .collect();

        dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

        for dir in dirs {
            if let Ok(mut entries) = fs::read_dir(&dir) {
                if entries.next().is_none() {
                    let _ = fs::remove_dir(&dir);
                }
            }
        }
    }

    fn remove_output_for_path(&self, path: &Path) {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.eq_ignore_ascii_case("_layout.html") {
                return;
            }
        }

        let rel_path = if let Ok(rel) = path.strip_prefix(&self.src_dir) {
            rel.to_path_buf()
        } else if let Some(name) = path.file_name() {
            PathBuf::from(name)
        } else {
            return;
        };

        let dest = self.out_dir.join(&rel_path);
        if !dest.exists() {
            return;
        }

        let result = if dest.is_dir() {
            fs::remove_dir_all(&dest)
        } else {
            fs::remove_file(&dest)
        };

        match result {
            Ok(_) => println!("[Cleanup] Removed {}", rel_path.display()),
            Err(e) => eprintln!("[Error] Failed to remove {}: {}", rel_path.display(), e),
        }
    }

    fn expected_output_set(&self) -> HashSet<PathBuf> {
        let mut expected = HashSet::new();

        for entry in WalkDir::new(&self.src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let rel = match path.strip_prefix(&self.src_dir) {
                Ok(p) => p.to_path_buf(),
                Err(_) => continue,
            };

            let is_html = rel
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("html"))
                .unwrap_or(false);

            if is_html {
                if rel
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case("_layout.html"))
                    .unwrap_or(false)
                {
                    continue;
                }
            }

            expected.insert(rel);
        }

        expected
    }

    fn file_hash_equal(&self, a: &Path, b: &Path) -> bool {
        let hash_a = self.file_hash(a);
        let hash_b = self.file_hash(b);
        hash_a == hash_b
    }

    fn file_hash(&self, path: &Path) -> Vec<u8> {
        let mut hasher = Sha256::new();
        if let Ok(mut file) = fs::File::open(path) {
            let mut buffer = [0; 8192];
            while let Ok(n) = file.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
        }
        hasher.finalize().to_vec()
    }
}
