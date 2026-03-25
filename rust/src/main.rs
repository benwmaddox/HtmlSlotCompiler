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
struct LayoutData {
    html: String,
    slots: Vec<SlotSpec>,
    layout_names: HashSet<String>,
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
}

#[derive(Debug, Clone)]
struct ExtractedPageSlot {
    tag: String,
    attributes: HashMap<String, String>,
    original_html: Option<String>,
    closing_style: SlotClosingStyle,
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

fn set_attribute_on_tag(tag_fragment: &str, attr: &str, value: &str) -> String {
    let without_attr = strip_attribute(tag_fragment, attr);
    let trimmed = without_attr.trim_end();

    let (base, closing) = if let Some(base) = trimmed.strip_suffix("/>") {
        (base.trim_end(), " />")
    } else if let Some(base) = trimmed.strip_suffix('>') {
        (base.trim_end(), ">")
    } else {
        (trimmed, "")
    };

    let mut result = base.to_string();
    if !result.ends_with(' ') {
        result.push(' ');
    }
    result.push_str(attr);
    result.push_str("=\"");
    result.push_str(&value.replace('"', "&quot;"));
    result.push('"');
    result.push_str(closing);
    result
}

fn include_tag_regex() -> regex::Regex {
    regex::Regex::new(
        r#"(?is)<include\b[^>]*\bsrc\s*=\s*["']([^"']+)["'][^>]*?(?:/\s*>|>\s*</include\s*>)"#,
    )
    .unwrap()
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

    let src_dir = src_dir_path
        .canonicalize()
        .unwrap_or_else(|_| src_dir_path.to_path_buf());
    let out_dir = Path::new(out_dir_arg).to_path_buf();
    let compiler = Compiler {
        src_dir: src_dir.clone(),
        out_dir: out_dir.clone(),
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

        let mut overall_ok = true;
        let src_dir_canonical = self
            .src_dir
            .canonicalize()
            .unwrap_or_else(|_| self.src_dir.clone());

        let mut full_rebuild = changed_paths.is_none();
        if let Some(paths) = changed_paths {
            if paths.is_empty() {
                full_rebuild = true;
            } else if paths.iter().any(|path| self.path_missing_with_retry(path)) {
                full_rebuild = true;
            } else if paths.iter().any(|path| self.is_layout_file(path)) {
                full_rebuild = true;
            }
        }

        if !full_rebuild {
            for path in self.collect_page_paths() {
                let rel_path = match path.strip_prefix(&self.src_dir) {
                    Ok(rel) => rel,
                    Err(_) => continue,
                };
                if !self.out_dir.join(rel_path).exists() {
                    full_rebuild = true;
                    break;
                }
            }
        }

        if !full_rebuild {
            if let Some(paths) = changed_paths {
                if paths.iter().any(|path| self.is_component_html(path)) {
                    full_rebuild = true;
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
            page_paths = self.collect_page_paths();
        } else if let Some(paths) = changed_paths {
            let mut seen = HashSet::new();
            for path in paths {
                if let Some(page_path) =
                    self.normalize_watch_path(path, src_dir_canonical.as_path())
                {
                    if seen.insert(page_path.clone()) {
                        page_paths.push(page_path);
                    }
                }
            }
        }

        let mut layout_cache = HashMap::new();
        for path in page_paths {
            let rel_path = match path.strip_prefix(&self.src_dir) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };
            let display_path = rel_path.display().to_string();

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

            let layout = match self.layout_for_page(&path, &mut layout_cache) {
                Ok(layout) => layout,
                Err(e) => {
                    eprintln!("[Error] {}: {}", display_path, e);
                    overall_ok = false;
                    continue;
                }
            };

            let expanded_page_html = match self.expand_includes_in_html(
                &page_html,
                path.parent().unwrap_or(self.src_dir.as_path()),
                &mut Vec::new(),
            ) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("[Error] {}: {}", display_path, e);
                    overall_ok = false;
                    continue;
                }
            };

            let page_doc = parse_html().one(page_html.clone());
            let expanded_page_doc = parse_html().one(expanded_page_html);

            // Extract page slots with metadata for normalization
            let mut raw_page_slots: HashMap<String, ExtractedPageSlot> = HashMap::new();
            let mut expanded_inner_html_by_slot: HashMap<String, String> = HashMap::new();
            let mut page_slot_order: Vec<String> = Vec::new();

            for element in page_doc.select("[for-slot]").unwrap() {
                let node = element.as_node();
                let attrs_ref = node.as_element().unwrap().attributes.borrow();

                if let Some(slot_name) = attrs_ref.get("for-slot") {
                    if raw_page_slots.contains_key(slot_name) {
                        continue;
                    }

                    let slot_name_string = slot_name.to_string();
                    let tag_name = node.as_element().unwrap().name.local.to_string();

                    let mut attributes = HashMap::new();
                    for (attr_name, attr_value) in attrs_ref.map.iter() {
                        attributes.insert(attr_name.local.to_string(), attr_value.value.clone());
                    }

                    let outer_html = self.get_outer_html(node);
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

                    page_slot_order.push(slot_name_string.clone());
                    raw_page_slots.insert(
                        slot_name_string,
                        ExtractedPageSlot {
                            tag: tag_name,
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

            for element in expanded_page_doc.select("[for-slot]").unwrap() {
                let node = element.as_node();
                let attrs_ref = node.as_element().unwrap().attributes.borrow();

                if let Some(slot_name) = attrs_ref.get("for-slot") {
                    if expanded_inner_html_by_slot.contains_key(slot_name) {
                        continue;
                    }

                    expanded_inner_html_by_slot
                        .insert(slot_name.to_string(), self.get_inner_html(node));
                }
            }

            let mut page_slots: HashMap<String, PageSlotContent> = HashMap::new();
            for (slot_name, raw_slot) in &raw_page_slots {
                page_slots.insert(
                    slot_name.clone(),
                    PageSlotContent {
                        tag: raw_slot.tag.clone(),
                        inner_html: expanded_inner_html_by_slot
                            .get(slot_name)
                            .cloned()
                            .unwrap_or_default(),
                        attributes: raw_slot.attributes.clone(),
                        original_html: raw_slot.original_html.clone(),
                        closing_style: raw_slot.closing_style,
                    },
                );
            }

            // Check for unknown slots
            let mut extra = Vec::new();
            for slot_name in page_slots.keys() {
                if !layout.layout_names.contains(slot_name) {
                    extra.push(slot_name.clone());
                }
            }

            if !extra.is_empty() {
                println!(
                    "[Error] {} has unknown slots: {}",
                    display_path,
                    extra.join(", ")
                );
                overall_ok = false;
                continue;
            }

            let expected_order: Vec<String> = layout
                .slots
                .iter()
                .filter(|slot| page_slots.contains_key(&slot.name))
                .map(|slot| slot.name.clone())
                .collect();
            let order_changed = page_slot_order != expected_order;

            let mut page_slots_for_merge = page_slots.clone();
            let mut missing_slots = Vec::new();
            for slot in &layout.slots {
                if !page_slots_for_merge.contains_key(&slot.name) {
                    missing_slots.push(slot.name.clone());
                    page_slots_for_merge
                        .insert(slot.name.clone(), self.default_slot_provider(slot));
                }
            }

            if !missing_slots.is_empty() {
                println!(
                    "[Normalize] Added missing slots in {}: {}",
                    display_path,
                    missing_slots.join(", ")
                );
            }

            if order_changed {
                println!(
                    "[Normalize] Reordered slots to match layout for {}",
                    display_path
                );
            }

            let uses_crlf = page_html.contains("\r\n");
            let had_trailing_newline = page_html.ends_with('\n') || page_html.ends_with("\r\n");

            let mut normalized_blocks = Vec::new();
            for slot in &layout.slots {
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

            if (order_changed || !missing_slots.is_empty())
                && normalized_compare != original_compare
            {
                let mut final_text = normalized_compare.clone();
                if had_trailing_newline {
                    final_text.push('\n');
                }
                if uses_crlf {
                    final_text = final_text.replace("\n", "\r\n");
                }

                match write_if_changed(&path, &final_text) {
                    Ok(true) => {
                        println!("[Normalize] Wrote {}", display_path);
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
            let mut output_html = layout.html.clone();

            for slot in &layout.slots {
                if let Some(content) = page_slots_for_merge.get(&slot.name) {
                    output_html = self.merge_slot_string(&output_html, slot, content);
                }
            }

            let dest_path = self.out_dir.join(&rel_path);
            let _ = fs::create_dir_all(dest_path.parent().unwrap());
            match write_if_changed(&dest_path, &output_html) {
                Ok(true) => println!("✔  Built {}", display_path),
                Ok(false) => println!("- Built {} (unchanged)", display_path),
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

    fn collect_page_paths(&self) -> Vec<PathBuf> {
        let mut page_paths = Vec::new();

        for entry in WalkDir::new(&self.src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path().to_path_buf();
            if self.is_page_html(&path) {
                page_paths.push(path);
            }
        }

        page_paths.sort();
        page_paths
    }

    fn layout_for_page(
        &self,
        page_path: &Path,
        layout_cache: &mut HashMap<PathBuf, LayoutData>,
    ) -> Result<LayoutData, String> {
        let layout_path = self.resolve_layout_path(page_path).ok_or_else(|| {
            let rel = page_path
                .strip_prefix(&self.src_dir)
                .unwrap_or(page_path)
                .display()
                .to_string();
            format!("Missing _layout.html for {}", rel)
        })?;

        let cache_key = layout_path
            .canonicalize()
            .unwrap_or_else(|_| layout_path.clone());
        if let Some(layout) = layout_cache.get(&cache_key) {
            return Ok(layout.clone());
        }

        let layout = self.load_layout_data(&layout_path)?;
        layout_cache.insert(cache_key, layout.clone());
        Ok(layout)
    }

    fn resolve_layout_path(&self, page_path: &Path) -> Option<PathBuf> {
        let mut current = page_path.parent()?;

        loop {
            if !current.starts_with(&self.src_dir) {
                return None;
            }

            let candidate = current.join("_layout.html");
            if candidate.exists() {
                return Some(candidate);
            }

            if current == self.src_dir {
                return None;
            }

            current = current.parent()?;
        }
    }

    fn load_layout_data(&self, layout_path: &Path) -> Result<LayoutData, String> {
        let layout_html = self.expand_includes_in_file(layout_path)?;
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
            let rel = layout_path
                .strip_prefix(&self.src_dir)
                .unwrap_or(layout_path)
                .display()
                .to_string();
            println!("[Warn] No slots in {}. Nothing to merge.", rel);
        }

        let layout_names = slots.iter().map(|slot| slot.name.clone()).collect();
        Ok(LayoutData {
            html: layout_html,
            slots,
            layout_names,
        })
    }

    fn is_layout_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("_layout.html"))
            .unwrap_or(false)
    }

    fn normalize_watch_path(&self, path: &Path, src_dir_canonical: &Path) -> Option<PathBuf> {
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

        if self.is_layout_file(&candidate) {
            return None;
        }

        if !self.is_page_html(&candidate) {
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

    fn is_page_html(&self, path: &Path) -> bool {
        if !self.is_html_file(path) {
            return false;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("_layout.html"))
            .unwrap_or(false)
        {
            return false;
        }

        self.html_has_slot_providers(path)
    }

    fn is_component_html(&self, path: &Path) -> bool {
        self.is_html_file(path)
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("_layout.html"))
                .unwrap_or(false)
            && !self.html_has_slot_providers(path)
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

    fn html_has_slot_providers(&self, path: &Path) -> bool {
        let html = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return false,
        };

        let doc = parse_html().one(html);
        doc.select("[for-slot]")
            .ok()
            .and_then(|mut nodes| nodes.next())
            .is_some()
    }

    fn expand_includes_in_file(&self, path: &Path) -> Result<String, String> {
        self.expand_includes_from_file(path, &mut Vec::new())
    }

    fn expand_includes_from_file(
        &self,
        path: &Path,
        stack: &mut Vec<PathBuf>,
    ) -> Result<String, String> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Failed to read include {}: {}", path.display(), e))?;

        if let Some(index) = stack.iter().position(|entry| *entry == canonical) {
            let mut chain = stack[index..]
                .iter()
                .map(|entry| entry.display().to_string())
                .collect::<Vec<_>>();
            chain.push(canonical.display().to_string());
            return Err(format!("Include cycle detected: {}", chain.join(" -> ")));
        }

        let html = fs::read_to_string(&canonical)
            .map_err(|e| format!("Failed to read include {}: {}", canonical.display(), e))?;

        stack.push(canonical.clone());
        let expanded = self.expand_includes_in_html(
            &html,
            canonical.parent().unwrap_or(self.src_dir.as_path()),
            stack,
        );
        stack.pop();
        expanded
    }

    fn expand_includes_in_html(
        &self,
        html: &str,
        current_dir: &Path,
        stack: &mut Vec<PathBuf>,
    ) -> Result<String, String> {
        let include_re = include_tag_regex();
        let mut result = String::with_capacity(html.len());
        let mut last_end = 0;

        for captures in include_re.captures_iter(html) {
            let matched = captures.get(0).unwrap();
            let src = captures.get(1).unwrap().as_str();
            result.push_str(&html[last_end..matched.start()]);

            let include_path = current_dir.join(src);
            let expanded = self.expand_includes_from_file(&include_path, stack)?;
            result.push_str(&expanded);

            last_end = matched.end();
        }

        result.push_str(&html[last_end..]);
        Ok(result)
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
                    let opening_tag = format!("{}{}", without_mode.trim_end(), ending);

                    match slot.mode.as_str() {
                        mode if mode.starts_with("attr:") => {
                            let attr_name = &mode[5..];
                            if let Some(value) = content.attributes.get(attr_name) {
                                set_attribute_on_tag(&opening_tag, attr_name, value)
                            } else {
                                opening_tag
                            }
                        }
                        _ => opening_tag,
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
                        let tag_with_attr = set_attribute_on_tag(&opening_tag, attr_name, value);
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

            if self.is_html_file(path) && !self.is_page_html(path) {
                continue;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("html-slot-compiler-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_compiler(root: &Path) -> Compiler {
        let src_dir = root.join("src");
        let out_dir = root.join("dist");
        fs::create_dir_all(&src_dir).unwrap();

        Compiler {
            src_dir: src_dir.clone(),
            out_dir,
        }
    }

    #[test]
    fn expands_recursive_includes_and_skips_component_output() {
        let root = make_temp_dir("recursive-include");
        let compiler = make_compiler(&root);

        fs::create_dir_all(compiler.src_dir.join("components")).unwrap();
        fs::write(
            &compiler.src_dir.join("_layout.html"),
            r#"
<!DOCTYPE html>
<html>
  <body>
    <main slot="content"></main>
  </body>
</html>
"#,
        )
        .unwrap();

        fs::write(
            compiler.src_dir.join("components/card.html"),
            r#"<article class="card"><include src="badge.html" /></article>"#,
        )
        .unwrap();
        fs::write(
            compiler.src_dir.join("components/badge.html"),
            r#"<span class="badge">Included</span>"#,
        )
        .unwrap();
        fs::write(
            compiler.src_dir.join("index.html"),
            r#"<main for-slot="content"><include src="components/card.html" /></main>"#,
        )
        .unwrap();

        assert!(compiler.build_once(None));

        let built = fs::read_to_string(compiler.out_dir.join("index.html")).unwrap();
        assert!(built
            .contains(r#"<article class="card"><span class="badge">Included</span></article>"#));
        assert!(!compiler.out_dir.join("components/card.html").exists());
        assert!(!compiler.out_dir.join("components/badge.html").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_include_cycles() {
        let root = make_temp_dir("include-cycle");
        let compiler = make_compiler(&root);

        fs::create_dir_all(compiler.src_dir.join("components")).unwrap();
        let a_path = compiler.src_dir.join("components/a.html");
        let b_path = compiler.src_dir.join("components/b.html");

        fs::write(&a_path, r#"<include src="b.html" />"#).unwrap();
        fs::write(&b_path, r#"<include src="a.html" />"#).unwrap();

        let error = compiler.expand_includes_in_file(&a_path).unwrap_err();
        assert!(error.contains("Include cycle detected"));
        assert!(error.contains("a.html"));
        assert!(error.contains("b.html"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn attr_slot_mode_updates_meta_content_without_mangling_tag() {
        let root = make_temp_dir("attr-slot-meta");
        let compiler = make_compiler(&root);

        fs::write(
            &compiler.src_dir.join("_layout.html"),
            r#"
<!DOCTYPE html>
<html>
  <head>
    <meta slot="description" slot-mode="attr:content" content="" />
  </head>
  <body>
    <main slot="content"></main>
  </body>
</html>
"#,
        )
        .unwrap();

        fs::write(
            compiler.src_dir.join("index.html"),
            r#"
<meta for-slot="description" content="Synthetic benchmark page 001" />
<main for-slot="content"><p>Hello</p></main>
"#,
        )
        .unwrap();

        assert!(compiler.build_once(None));

        let built = fs::read_to_string(compiler.out_dir.join("index.html")).unwrap();
        assert!(built.contains(r#"<meta content="Synthetic benchmark page 001" />"#));
        assert!(!built.contains(r#"/ content=""#));
        assert_eq!(built.matches("content=").count(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn builds_nested_pages_with_the_closest_layout() {
        let root = make_temp_dir("nested-layout");
        let compiler = make_compiler(&root);
        let blog_dir = compiler.src_dir.join("blog");
        let post_dir = blog_dir.join("posts");
        fs::create_dir_all(&post_dir).unwrap();

        fs::write(
            &compiler.src_dir.join("_layout.html"),
            r#"
<!DOCTYPE html>
<html>
  <body>
    <header slot="header"></header>
    <main slot="content"></main>
  </body>
</html>
"#,
        )
        .unwrap();

        fs::write(
            blog_dir.join("_layout.html"),
            r#"
<!DOCTYPE html>
<html>
  <body class="blog-shell">
    <aside slot="header"></aside>
    <article slot="content"></article>
  </body>
</html>
"#,
        )
        .unwrap();

        fs::write(
            post_dir.join("post.html"),
            r#"
<section for-slot="header"><h1>Nested Post</h1></section>
<section for-slot="content"><p>Rendered with the nearest layout.</p></section>
"#,
        )
        .unwrap();

        assert!(compiler.build_once(None));

        let built = fs::read_to_string(compiler.out_dir.join("blog/posts/post.html")).unwrap();
        assert!(built.contains(r#"<body class="blog-shell">"#));
        assert!(built.contains(r#"<aside><h1>Nested Post</h1></aside>"#));
        assert!(built.contains(r#"<article><p>Rendered with the nearest layout.</p></article>"#));

        let _ = fs::remove_dir_all(root);
    }
}
