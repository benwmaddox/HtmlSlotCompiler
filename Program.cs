using System.Collections.Concurrent;
using System.Security.Cryptography;
using AngleSharp;
using AngleSharp.Dom;

static class Program
{
    static async Task<int> Main(string[] args)
    {
        var srcDir = args.FirstOrDefault(a => !a.StartsWith("--")) ?? "src";
        var outDir = args.Skip(1).FirstOrDefault(a => !a.StartsWith("--")) ?? "dist";
        var watch = args.Contains("--watch");

        if (!Directory.Exists(srcDir))
        {
            Console.Error.WriteLine($"[Error] Source directory not found: {srcDir}");
            return 1;
        }

        var layoutPath = Path.Combine(srcDir, "_layout.html");
        if (!File.Exists(layoutPath))
        {
            Console.Error.WriteLine($"[Error] Missing {layoutPath}");
            return 1;
        }

        var compiler = new SiteCompiler(srcDir, outDir, layoutPath);
        var ok = await compiler.BuildOnce();
        if (!watch) return ok ? 0 : 2;

        Console.WriteLine("[Watch] Watching for changes…");
        using var fsw = new FileSystemWatcher(srcDir)
        {
            IncludeSubdirectories = true,
            EnableRaisingEvents = true,
            Filter = "*.*"
        };

        var pending = new ConcurrentDictionary<string, byte>();
        var timer = new System.Timers.Timer(150) { AutoReset = false, Enabled = false };
        timer.Elapsed += async (_, _) =>
        {
            try
            {
                // debounce: rebuild once
                pending.Clear();
                await compiler.BuildOnce();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Error] {ex.Message}");
            }
        };

        FileSystemEventHandler onChange = (_, e) =>
        {
            if (e.FullPath.EndsWith(".tmp", StringComparison.OrdinalIgnoreCase)) return;
            pending[e.FullPath] = 1;
            timer.Stop();
            timer.Start();
        };

        fsw.Changed += onChange;
        fsw.Created += onChange;
        fsw.Deleted += onChange;
        fsw.Renamed += (_, __) => { timer.Stop(); timer.Start(); };

        await Task.Delay(Timeout.Infinite);
        // unreachable
#pragma warning disable CS0162
        return 0;
#pragma warning restore CS0162
    }
}

file sealed class SiteCompiler(string srcDir, string outDir, string layoutPath)
{
    private readonly string _src = srcDir;
    private readonly string _out = outDir;
    private readonly string _layoutPath = layoutPath;
    private readonly IBrowsingContext _ctx = BrowsingContext.New(Configuration.Default);

    public async Task<bool> BuildOnce()
    {
        Console.WriteLine($"[Build] {DateTime.Now:T}");
        Directory.CreateDirectory(_out);

        var layoutHtml = await File.ReadAllTextAsync(_layoutPath);
        var layoutDoc = await _ctx.OpenAsync(req => req.Content(layoutHtml));

        // Ordered slot spec from layout
        var slotSpecs = layoutDoc.All
            .Where(e => e.HasAttribute("slot"))
            .Select(e => new SlotSpec(
                Name: e.GetAttribute("slot")!,
                Mode: e.GetAttribute("slot-mode") ?? "html",
                LayoutTag: e.TagName.ToLowerInvariant(),
                InHead: e.GetAncestor("head") is not null
            ))
            .ToList();

        if (slotSpecs.Count == 0)
        {
            Console.WriteLine("[Warn] No slots in _layout.html. Nothing to merge.");
        }

        bool overallOk = true;

        foreach (var pagePath in Directory.EnumerateFiles(_src, "*.html", SearchOption.TopDirectoryOnly)
                     .Where(p => !Path.GetFileName(p).Equals("_layout.html", StringComparison.OrdinalIgnoreCase)))
        {
            var fileName = Path.GetFileName(pagePath);
            var pageHtml = await File.ReadAllTextAsync(pagePath);
            var pageDoc = await _ctx.OpenAsync(req => req.Content(pageHtml));

            // Gather for-slot elements (all of them)
            var pageSlots = pageDoc.All.Where(e => e.HasAttribute("for-slot")).ToList();
            var pageMap = pageSlots.GroupBy(e => e.GetAttribute("for-slot")!)
                                   .ToDictionary(g => g.Key, g => g.First());

            // Detect extras (error)
            var layoutNames = slotSpecs.Select(s => s.Name).ToHashSet(StringComparer.Ordinal);
            var extra = pageMap.Keys.Where(k => !layoutNames.Contains(k)).ToList();
            if (extra.Count > 0)
            {
                Console.WriteLine($"[Error] {fileName} has unknown slots: {string.Join(", ", extra)}");
                overallOk = false;
                // We still normalize/write back, but we will SKIP merging/build output
            }

            // Normalize: ensure presence & ordering for layout-defined slots
            bool changed = false;
            var normalizedHead = new List<IElement>();
            var normalizedBody = new List<IElement>();

            foreach (var spec in slotSpecs)
            {
                if (!pageMap.TryGetValue(spec.Name, out var el))
                {
                    // Auto-create an empty provider in correct container
                    el = CreateEmptyProvider(pageDoc, spec);
                    changed = true;
                    Console.WriteLine($"[AutoAdd] {fileName}: inserted missing slot '{spec.Name}'");
                }
                // assign into target container list by layout placement
                if (spec.InHead) normalizedHead.Add(el);
                else normalizedBody.Add(el);
            }

            // Rebuild page with normalized slot order, leaving non-slot content intact
            // Strategy: remove all existing for-slot nodes, then insert normalized
            var existingForSlots = pageDoc.All.Where(e => e.HasAttribute("for-slot")).ToList();
            if (!ListsEqualByRefOrder(existingForSlots.Where(e => (e.GetAncestor("head") != null)).ToList(), normalizedHead)
                || !ListsEqualByRefOrder(existingForSlots.Where(e => (e.GetAncestor("body") != null)).ToList(), normalizedBody))
            {
                changed = true;
            }

            foreach (var n in existingForSlots) n.Remove();

            // Ensure head/body
            if (pageDoc.Head is null)
                pageDoc.DocumentElement?.InsertBefore(pageDoc.CreateElement("head"), pageDoc.DocumentElement.FirstChild);
            if (pageDoc.Body is null)
                pageDoc.DocumentElement?.AppendChild(pageDoc.CreateElement("body"));

            foreach (var n in normalizedHead) pageDoc.Head!.AppendChild(n);
            foreach (var n in normalizedBody) pageDoc.Body!.AppendChild(n);

            if (changed)
            {
                await File.WriteAllTextAsync(pagePath, pageDoc.ToHtml());
                Console.WriteLine($"[Normalized] Updated {fileName}");
            }

            // If extras exist, skip building this page but continue other files
            if (extra.Count > 0)
                continue;

            // Merge into layout for output
            var outDoc = await _ctx.OpenAsync(req => req.Content(layoutHtml));

            foreach (var spec in slotSpecs)
            {
                var layoutEl = outDoc.All.First(e => e.HasAttribute("slot") && e.GetAttribute("slot") == spec.Name);
                // After normalization, provider must exist (either in head or body)
                var provider = (outDoc.Head?.QuerySelector($"[for-slot='{spec.Name}']") ??
                                outDoc.Body?.QuerySelector($"[for-slot='{spec.Name}']")) ?? null;

                // Provider comes from the PAGE, not from the layout clone; we need it from pageDoc.
                provider = pageDoc.All.First(e => e.HasAttribute("for-slot") && e.GetAttribute("for-slot") == spec.Name);

                switch (spec.Mode)
                {
                    case "text":
                        layoutEl.TextContent = provider.TextContent;
                        break;
                    default:
                        if (spec.Mode.StartsWith("attr:", StringComparison.Ordinal))
                        {
                            var attrName = spec.Mode.Substring(5);
                            var val = provider.GetAttribute(attrName);
                            if (val is null)
                                throw new Exception($"{fileName}: slot '{spec.Name}' expected attribute '{attrName}'");
                            layoutEl.SetAttribute(attrName, val);
                        }
                        else
                        {
                            layoutEl.InnerHtml = provider.InnerHtml;
                        }
                        break;
                }
            }

            var destPath = Path.Combine(_out, fileName);
            Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
            await File.WriteAllTextAsync(destPath, outDoc.ToHtml());
            Console.WriteLine($"✔ Built {fileName}");
        }

        await CopyAssetsDiff(_src, _out);
        Console.WriteLine("[Build] Complete.\n");
        return overallOk;
    }

    private static bool ListsEqualByRefOrder(IReadOnlyList<IElement> a, IReadOnlyList<IElement> b)
    {
        if (a.Count != b.Count) return false;
        for (int i = 0; i < a.Count; i++)
            if (!ReferenceEquals(a[i], b[i])) return false;
        return true;
    }

    private static IElement CreateEmptyProvider(IDocument pageDoc, SlotSpec spec)
    {
        // Heuristic: if layout tag is "title" → use <title>, if "meta" → <meta>, else <section>
        string tag = spec.LayoutTag switch
        {
            "title" => "title",
            "meta"  => "meta",
            _       => "section",
        };
        var el = pageDoc.CreateElement(tag);
        el.SetAttribute("for-slot", spec.Name);

        // If this slot is attr-mode, ensure the attribute exists (empty)
        if (spec.Mode.StartsWith("attr:", StringComparison.Ordinal))
        {
            var attrName = spec.Mode[5..];
            if (!el.HasAttribute(attrName)) el.SetAttribute(attrName, "");
        }

        return el;
    }

    private static async Task CopyAssetsDiff(string src, string dst)
    {
        var srcFiles = Directory.EnumerateFiles(src, "*", SearchOption.AllDirectories)
            .Where(f => !f.EndsWith(".html", StringComparison.OrdinalIgnoreCase) &&
                        !Path.GetFileName(f).Equals("_layout.html", StringComparison.OrdinalIgnoreCase));

        foreach (var s in srcFiles)
        {
            var rel = Path.GetRelativePath(src, s);
            var d = Path.Combine(dst, rel);
            Directory.CreateDirectory(Path.GetDirectoryName(d)!);

            if (!File.Exists(d) || !await FileHashEqual(s, d))
            {
                File.Copy(s, d, overwrite: true);
                Console.WriteLine($"📁 Copied {rel}");
            }
        }
    }

    private static async Task<bool> FileHashEqual(string a, string b)
    {
        var fa = new FileInfo(a);
        var fb = new FileInfo(b);
        if (fa.Length != fb.Length) return false;

        using var sha = SHA256.Create();
        await using var sa = File.OpenRead(a);
        await using var sb = File.OpenRead(b);
        var ha = await sha.ComputeHashAsync(sa);
        var hb = await sha.ComputeHashAsync(sb);
        return ha.AsSpan().SequenceEqual(hb);
    }

    private readonly record struct SlotSpec(string Name, string Mode, string LayoutTag, bool InHead);
}

file static class DomExtensions
{
    public static IElement? GetAncestor(this IElement el, string tagLower)
    {
        for (var p = el.ParentElement; p is not null; p = p.ParentElement)
            if (p.TagName.Equals(tagLower, StringComparison.OrdinalIgnoreCase)) return p;
        return null;
    }
}
