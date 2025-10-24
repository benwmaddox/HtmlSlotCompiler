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
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct SlotSpec {
    name: String,
    mode: String,
    layout_tag: String,
    self_closing: bool,
}

#[derive(Debug, Clone)]
struct PageSlotContent {
    tag: String,
    inner_html: String,
    attributes: HashMap<String, String>,
    original_html: Option<String>,
    self_closing: bool,
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
    fn render(&self, prefer_self_closing: bool) -> String {
        if let Some(original) = &self.original_html {
            original.clone()
        } else {
            let should_self_close = self.self_closing || prefer_self_closing;
            Self::build_markup(
                &self.tag,
                &self.attributes,
                &self.inner_html,
                should_self_close,
            )
        }
    }

    fn build_markup(
        tag: &str,
        attributes: &HashMap<String, String>,
        inner_html: &str,
        self_closing: bool,
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

        if self_closing {
            format!("<{}{} />", tag, attr_string)
        } else {
            format!("<{}{}>{}</{}>", tag, attr_string, inner_html, tag)
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    VOID_TAGS.contains(&lower.as_str())
}

fn detect_layout_self_closing(layout_html: &str, tag: &str, slot_name: &str) -> bool {
    let pattern = format!(
        r#"(?is)<{tag}\b[^>]*\bslot\s*=\s*["']{slot}["'][^>]*>"#,
        tag = regex::escape(tag),
        slot = regex::escape(slot_name)
    );

    if let Ok(re) = regex::Regex::new(&pattern) {
        if let Some(mat) = re.find(layout_html) {
            let snippet = mat.as_str().trim_end();
            if snippet.ends_with("/>") {
                return true;
            }
            if snippet.ends_with(">") && snippet.contains("/>") {
                return true;
            }
        }
    }

    is_void_element(tag)
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

    let src_dir = args.get(1).map(|s| s.as_str()).unwrap_or("src");
    let out_dir = args.get(2).map(|s| s.as_str()).unwrap_or("dist");
    let watch = args.get(3).map(|s| s.as_str()) == Some("--watch");

    if !Path::new(src_dir).exists() {
        eprintln!("[Error] Source directory not found: {}", src_dir);
        std::process::exit(1);
    }

    let layout_path = Path::new(src_dir).join("_layout.html");
    if !layout_path.exists() {
        eprintln!("[Error] Missing {}", layout_path.display());
        std::process::exit(1);
    }

    let compiler = Compiler {
        src_dir: src_dir.into(),
        out_dir: out_dir.into(),
        layout_path,
    };

    let ok = compiler.build_once();
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

    let pending = Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
    let pending_clone = Arc::clone(&pending);

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        match res {
            Ok(event) => {
                for path in event.paths {
                    if !path.to_string_lossy().ends_with(".tmp") {
                        let _ = tx.send(path);
                    }
                }
            }
            Err(_) => {}
        }
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
                pending_clone.lock().unwrap().insert(path.to_string_lossy().to_string());
                timer_active = true;
                last_build = std::time::Instant::now();
            }
            Err(_) => {
                if timer_active && last_build.elapsed() >= Duration::from_millis(150) {
                    pending_clone.lock().unwrap().clear();
                    let compiler = Compiler {
                        src_dir: src_dir_clone.clone(),
                        out_dir: out_dir_clone.clone(),
                        layout_path: layout_path_clone.clone(),
                    };
                    compiler.build_once();
                    timer_active = false;
                }
            }
        }
    }
}

impl Compiler {
    fn build_once(&self) -> bool {
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
            let self_closing =
                detect_layout_self_closing(&layout_html, &layout_tag, &name);

            slots.push(SlotSpec {
                name,
                mode,
                layout_tag,
                self_closing,
            });
        }

        if slots.is_empty() {
            println!("[Warn] No slots in _layout.html. Nothing to merge.");
        }

        let mut overall_ok = true;

        // Process each HTML page
        let entries = match fs::read_dir(&self.src_dir) {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("[Error] {}", e);
                return false;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.is_dir() || !path.extension().map_or(false, |ext| ext == "html") {
                continue;
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            if file_name == "_layout.html" {
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
                            attributes.insert(
                                attr_name.local.to_string(),
                                attr_value.value.clone(),
                            );
                        }

                        let outer_html = self.get_outer_html(node);
                        let inner_html = self.get_inner_html(node);

                        page_slot_order.push(slot_name_string.clone());
                        let trimmed_outer = outer_html.trim_end();
                        let self_closing = trimmed_outer.ends_with("/>")
                            && !trimmed_outer.contains("</");

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
                                self_closing,
                            },
                        );
                    }
                }
            }

            // Check for unknown slots
            let layout_names: HashSet<_> = slots.iter().map(|s| s.name.clone()).collect();
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
                    page_slots_for_merge.insert(
                        slot.name.clone(),
                        self.default_slot_provider(slot),
                    );
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
                println!("[Normalize] Reordered slots to match layout for {}", file_name);
            }

            let uses_crlf = page_html.contains("\r\n");
            let had_trailing_newline =
                page_html.ends_with('\n') || page_html.ends_with("\r\n");

            let mut normalized_blocks = Vec::new();
            for slot in &slots {
                if let Some(content) = page_slots_for_merge.get(&slot.name) {
                    normalized_blocks.push(content.render(slot.self_closing));
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

                if let Err(e) = fs::write(&path, &final_text) {
                    eprintln!("[Error] {}", e);
                    overall_ok = false;
                    continue;
                } else {
                    println!("[Normalize] Wrote {}", file_name);
                }
            }

            // Build output by merging page slots into layout (string-based to preserve whitespace)
            let mut output_html = layout_html.clone();

            for slot in &slots {
                if let Some(content) = page_slots_for_merge.get(&slot.name) {
                    output_html =
                        self.merge_slot_string(&output_html, slot, content);
                }
            }

            let dest_path = self.out_dir.join(&file_name);
            let _ = fs::create_dir_all(dest_path.parent().unwrap());
            if let Err(e) = fs::write(&dest_path, &output_html) {
                eprintln!("[Error] {}", e);
                overall_ok = false;
                continue;
            }

            println!("✔ Built {}", file_name);
        }

        self.copy_assets_diff();
        let elapsed_ms = start.elapsed().as_millis();
        println!(
            "[Build] Complete in {} ms.\n",
            format_with_commas(elapsed_ms)
        );

        overall_ok
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
        String::from_utf8_lossy(&result).trim().to_string()
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
            self_closing: slot.self_closing,
        }
    }

    fn merge_slot_string(
        &self,
        html: &str,
        slot: &SlotSpec,
        content: &PageSlotContent,
    ) -> String {
        if slot.self_closing {
            let pattern = format!(
                r#"(?is)(<{tag}\b[^>]*\bslot\s*=\s*["']{name}["'][^>]*)(\s*/?>)"#,
                tag = regex::escape(&slot.layout_tag),
                name = regex::escape(&slot.name)
            );

            let re = regex::Regex::new(&pattern).unwrap();

            return re
                .replace(html, |caps: &regex::Captures| {
                    let mut opening_tag = caps[1].to_string();
                    let ending = &caps[2];

                    opening_tag = opening_tag
                        .replace(&format!(r#" slot="{}""#, slot.name), "")
                        .replace(
                            &format!(r#" slot-mode="{}""#, slot.mode),
                            "",
                        )
                        .replace(r#" slot-mode="text""#, "")
                        .replace(r#" slot-mode="html""#, "");

                    let opening_tag = opening_tag.trim_end();

                    match slot.mode.as_str() {
                        mode if mode.starts_with("attr:") => {
                            let attr_name = &mode[5..];
                            if let Some(value) = content.attributes.get(attr_name)
                            {
                                let mut builder = opening_tag.to_string();
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
            r#"(<{}\s[^>]*slot="{}[^>]*>)(.*?)(</{}>)"#,
            slot.layout_tag, slot.name, slot.layout_tag
        );

        let re = regex::Regex::new(&pattern).unwrap();

        re.replace(html, |caps: &regex::Captures| {
            let opening_tag = &caps[1];
            let closing_tag = &caps[3];

            // Remove slot and slot-mode attributes from opening tag
            let cleaned_tag = opening_tag
                .replace(&format!(r#" slot="{}""#, slot.name), "")
                .replace(&format!(r#" slot-mode="{}""#, slot.mode), "")
                .replace(r#" slot-mode="text""#, "")
                .replace(r#" slot-mode="html""#, "");

            match slot.mode.as_str() {
                "text" => {
                    // For text mode, insert content as plain text
                    format!("{}{}{}", cleaned_tag, &content.inner_html, closing_tag)
                }
                mode if mode.starts_with("attr:") => {
                    // For attr mode, copy attribute value from the provider element
                    let attr_name = &mode[5..];

                    if let Some(value) = content.attributes.get(attr_name) {
                        let tag_with_attr = cleaned_tag
                            .replace(">", &format!(r#" {}="{}">"#, attr_name, value));
                        format!("{}{}", tag_with_attr, closing_tag)
                    } else {
                        format!("{}{}", cleaned_tag, closing_tag)
                    }
                }
                _ => {
                    // For html mode (default), insert content as HTML
                    format!("{}{}{}", cleaned_tag, &content.inner_html, closing_tag)
                }
            }
        }).to_string()
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
