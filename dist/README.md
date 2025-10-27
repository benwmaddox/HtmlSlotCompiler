# HtmlSlotCompiler Distribution

`HtmlSlotCompiler.exe` is the standalone build of the HtmlSlotCompiler CLI, packaged for embedding in other repositories.

## Quick Use
- Ensure your source folder contains `_layout.html` plus any number of pages that provide `for-slot` fragments.
- Run `HtmlSlotCompiler.exe <source_dir> <output_dir>` to normalize HTML and emit the compiled site.
- Add `--watch` to rebuild automatically while editing.

## Slot Modes
- Omit `slot-mode` (default `html`) to merge provider markup into the layout element.
- Use `slot-mode="text"` to insert the provider content as plain text.
- Use `slot-mode="attr:name"` to copy the provider's `name` attribute onto the layout element, leaving the element body untouched.

```html
<!-- _layout.html -->
<head>
  <title slot="title" slot-mode="text"></title>
  <meta slot="description" slot-mode="attr:content" />
</head>
<body>
  <main slot="content"></main>
</body>
```

```html
<!-- page.html -->
<title for-slot="title">Docs</title>
<meta for-slot="description" content="Docs summary" />
<section for-slot="content">
  <p>Body</p>
</section>
```

For full documentation, examples, and updates visit https://github.com/benwmaddox/HtmlSlotCompiler.
