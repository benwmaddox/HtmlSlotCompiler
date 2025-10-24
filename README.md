# HtmlSlotCompiler

A layout-driven static HTML compiler that enforces structure instead of templating.
It merges pages with a shared `_layout.html`, auto-normalizes page order, and ensures all pages conform to the same schema.

**No templating. No includes. Just HTML.**

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
Your source files (`src/*.html`) will be normalized automatically to match `_layout.html`.

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
