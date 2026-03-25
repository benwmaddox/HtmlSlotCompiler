# HtmlSlotCompiler

A layout-driven static HTML compiler that enforces structure instead of templating.
It merges pages with a shared `_layout.html`, auto-normalizes page order, and ensures all pages conform to the same schema.

**No templating. Just HTML with slot-based pages and static includes.**

## 🚀 Quick Start

```bash
# 1. create your source folder
mkdir src
cd src

# 2. create a layout
echo "<html><body><header slot='header'></header><main slot='content'></main><footer slot='footer'></footer></body></html>" > _layout.html

# 3. create a page
echo "<section for-slot='content'><p>Hello world</p></section>" > index.html

# 4. build it
site-compiler src dist
```

Output goes into `dist/`.
Your source files (`src/**/*.html`) will be normalized automatically to match the nearest `_layout.html` found by walking up from each page.

## 🧩 Concept

`_layout.html` defines slots, each with an ordered name:

```html
<html>
  <head>
    <title slot="title" slot-mode="text"></title>
    <meta name="description" slot="description" slot-mode="attr:content" />
  </head>
  <body>
    <header slot="header"></header>
    <main slot="content"></main>
    <footer slot="footer"></footer>
  </body>
</html>
```

Each page defines providers for these slots:

```html
<title for-slot="title">About Us</title>
<meta for-slot="description" content="Learn about our team." />
<section for-slot="header"><h1>About</h1></section>
<section for-slot="content"><p>We build cool things.</p></section>
<section for-slot="footer"><p>© 2025 Example</p></section>
```

When compiled:

- every slot is present,
- every slot is in the same order as `_layout.html`,
- extra slots are errors,
- missing ones are auto-added,
- normalized source is written back if changed.

Pages can live in nested folders. Each page uses the closest `_layout.html` in its own folder or an ancestor folder under the source root.

## Components

Static HTML fragments can be reused with:

```html
<include src="components/hero.html" />
```

Include paths are resolved relative to the file that contains the include tag, and nested includes are expanded recursively.

Component files are treated as fragments, not pages, so they are not emitted into `dist/` unless they are referenced by a normal asset pipeline outside the compiler.

## 🧠 Philosophy

Most static site generators (Astro, Eleventy, Jekyll, Hugo) emphasize flexibility — loops, includes, logic, data merging.

**HtmlSlotCompiler** emphasizes the opposite: **structural consistency**.

It treats `_layout.html` as a schema.
All pages are enforced to match it exactly.

This makes it ideal for:

- Mass-generated sites (e.g. hundreds of local business pages)
- AI-generated HTML cleanup
- Design-first workflows where editors use real HTML tools
- Offline or AOT builds (compiles to a single native binary)

## ⚙️ Features

| Feature                    | Description                                          |
| -------------------------- | ---------------------------------------------------- |
| ✅ Pure HTML               | No templating syntax, no front matter                |
| ✅ Strict enforcement      | Missing → added, out-of-order → reordered            |
| ✅ Errors on extras        | Keeps schema clean                                   |
| ✅ Proper DOM manipulation | Uses kuchiki for correct HTML parsing                |
| ✅ Single binary           | Compiles to 1.5MB native executable (Rust)           |
| ✅ Smart asset copying     | Copies CSS/JS/images only if changed (SHA256 hash)   |
| ✅ Watch mode              | `--watch` flag for continuous builds with debouncing |
| ⚡ Fast                    | 29ms build time for 2 pages                          |

## 🧰 Usage

```bash
# build once
site-compiler src dist

# build and watch for changes
site-compiler src dist --watch
```

### Behavior

| Case                | Result                                                 |
| ------------------- | ------------------------------------------------------ |
| Missing slot        | Auto-added empty `<section for-slot="name"></section>` |
| Wrong order         | Reordered to match layout                              |
| Extra slot          | Error (page skipped)                                   |
| Different structure | Source HTML rewritten in normalized order              |
| Assets changed      | Copied with hash comparison                            |

## 🏗️ Build & Publish

```bash
cd rust
cargo build --release
```

Produces a single 1.5MB executable in `rust/target/release/site-compiler.exe` (Windows) or `site-compiler` (Unix).

## 🌙 Nightly Releases

The repo includes a GitHub Actions nightly release workflow in `.github/workflows/nightly-release.yml`.

- It runs on a nightly schedule and via manual dispatch.
- It builds release archives for Windows, Linux, and macOS.
- It only publishes when the current `master` HEAD differs from the previous `nightly` tag.
- It updates the `nightly` prerelease in GitHub with fresh cross-platform artifacts.

## 🧮 Comparison

| Feature                  | HtmlSlotCompiler          | Eleventy           | Astro           | Jekyll/Hugo    |
| ------------------------ | ------------------------- | ------------------ | --------------- | -------------- |
| Templating syntax        | ❌ none                   | ✅ Liquid/Nunjucks | ✅ JSX/MDX      | ✅ Liquid/Go   |
| Strict layout order      | ✅                        | ⚠️ optional        | ⚠️              | ⚠️             |
| Auto-normalize source    | ✅                        | ❌                 | ❌              | ❌             |
| Dynamic data             | ❌                        | ✅                 | ✅              | ✅             |
| Startup speed            | ⚡ instant                | 🐢 slow            | 🐇 fast         | ⚙️ medium      |
| HTML validity in editors | ✅ 100%                   | ⚠️ often broken    | ❌              | ⚠️             |
| Ideal use                | schema-driven static HTML | content blogs      | component sites | markdown blogs |

## 🧩 Example Repo Layout

```
src/
  _layout.html
  index.html
  about.html
  blog/
    _layout.html
    posts/
      launch.html
  css/
    site.css
  img/
    logo.png
dist/
```

## 🧠 Why It's Useful

This tool enforces HTML consistency for large or machine-generated sites.
If you generate hundreds of pages automatically, it ensures:

- every page matches the canonical layout structure,
- broken markup is corrected,
- editors can safely tweak output directly,
- and builds always produce clean HTML with identical layout order.

## 📜 License

Use freely, but you cannot modify or redistribute altered versions. See `LICENSE.txt`.

---

Built by Ben Maddox.

# Run samples on Windows:

```
rust/target/release/site-compiler.exe sample/src sample/dist --watch
```

## Development

If `cargo` is not already on your `PATH`, source your local Rust environment first:

```bash
source "$HOME/.cargo/env"
```

Standard local validation:

```bash
./Scripts/validate.sh
```

That runs Rust formatting, `cargo check`, `cargo test`, and a smoke test that compiles `sample/src/` into a temporary output directory and verifies the generated files.

## Night Shift Workflow

This repository now includes a Night Shift style agent workflow. The canonical router is [AGENTS.md](AGENTS.md), with detailed operating docs in `Docs/`.

The intended loop is:

1. keep `Docs/BUGS.md`, `Docs/TODOS.md`, and `Specs/` current
2. run `./Scripts/nightshift.sh codex` or `./Scripts/nightshift.sh claude`
3. review the morning report and commit history before merging
