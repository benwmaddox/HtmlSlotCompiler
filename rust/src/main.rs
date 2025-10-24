use kuchiki::traits::*;
use kuchiki::{NodeRef, parse_html};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Read;
use walkdir::WalkDir;
use chrono::Local;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use notify::{Watcher, RecursiveMode};

#[derive(Debug, Clone)]
struct SlotSpec {
    name: String,
    mode: String,
    layout_tag: String,
    in_head: bool,
}

struct Compiler {
    src_dir: PathBuf,
    out_dir: PathBuf,
    layout_path: PathBuf,
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

            // Detect if in head
            let in_head = self.is_in_head(node);

            slots.push(SlotSpec {
                name,
                mode,
                layout_tag,
                in_head,
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

            // Extract page slots
            let mut page_map: HashMap<String, String> = HashMap::new();

            for element in page_doc.select("[for-slot]").unwrap() {
                let node = element.as_node();
                let attrs = node.as_element().unwrap().attributes.borrow();

                if let Some(slot_name) = attrs.get("for-slot") {
                    if !page_map.contains_key(slot_name) {
                        // Get inner HTML of this element
                        let inner_html = self.get_inner_html(node);
                        page_map.insert(slot_name.to_string(), inner_html);
                    }
                }
            }

            // Check for unknown slots
            let layout_names: std::collections::HashSet<_> =
                slots.iter().map(|s| s.name.clone()).collect();
            let mut extra = Vec::new();
            for slot_name in page_map.keys() {
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

            // Build output by merging page slots into layout (string-based to preserve whitespace)
            let mut output_html = layout_html.clone();

            for slot in &slots {
                if let Some(content) = page_map.get(&slot.name) {
                    output_html = self.merge_slot_string(&output_html, &slot, content);
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
        println!("[Build] Complete.\n");

        overall_ok
    }

    fn is_in_head(&self, node: &NodeRef) -> bool {
        // Walk up the tree to find if we're inside a <head> element
        let mut current = node.parent();
        while let Some(parent) = current {
            if let Some(element) = parent.as_element() {
                if element.name.local.to_string() == "head" {
                    return true;
                }
            }
            current = parent.parent();
        }
        false
    }

    fn get_inner_html(&self, node: &NodeRef) -> String {
        // Get the inner HTML by serializing all children
        let mut result = Vec::new();
        for child in node.children() {
            let mut child_html = Vec::new();
            child.serialize(&mut child_html).ok();
            result.extend(child_html);
        }
        String::from_utf8_lossy(&result).trim().to_string()
    }

    fn merge_slot_string(&self, html: &str, slot: &SlotSpec, content: &str) -> String {
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
                    format!("{}{}{}", cleaned_tag, content, closing_tag)
                }
                mode if mode.starts_with("attr:") => {
                    // For attr mode, extract attribute value from content and add to tag
                    let attr_name = &mode[5..];

                    // Simple attribute extraction
                    if let Some(attr_start) = content.find(&format!(r#"{}=""#, attr_name)) {
                        let value_start = attr_start + attr_name.len() + 2;
                        if let Some(value_end) = content[value_start..].find('"') {
                            let attr_value = &content[value_start..value_start + value_end];

                            // Insert attribute into opening tag
                            let tag_with_attr = cleaned_tag.replace(">", &format!(r#" {}="{}">"#, attr_name, attr_value));
                            format!("{}{}", tag_with_attr, closing_tag)
                        } else {
                            format!("{}{}", cleaned_tag, closing_tag)
                        }
                    } else {
                        format!("{}{}", cleaned_tag, closing_tag)
                    }
                }
                _ => {
                    // For html mode (default), insert content as HTML
                    format!("{}{}{}", cleaned_tag, content, closing_tag)
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
