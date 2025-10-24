package main

import (
	"bytes"
	"crypto/sha256"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/PuerkitoBio/goquery"
	"github.com/fsnotify/fsnotify"
)

type SlotSpec struct {
	Name      string
	Mode      string
	LayoutTag string
	InHead    bool
}

type Compiler struct {
	srcDir     string
	outDir     string
	layoutPath string
}

func main() {
	flag.Parse()
	args := flag.Args()

	srcDir := "src"
	outDir := "dist"
	watch := false

	if len(args) > 0 {
		srcDir = args[0]
	}
	if len(args) > 1 {
		outDir = args[1]
	}
	if len(args) > 2 && args[2] == "--watch" {
		watch = true
	}

	if _, err := os.Stat(srcDir); os.IsNotExist(err) {
		fmt.Fprintf(os.Stderr, "[Error] Source directory not found: %s\n", srcDir)
		os.Exit(1)
	}

	layoutPath := filepath.Join(srcDir, "_layout.html")
	if _, err := os.Stat(layoutPath); os.IsNotExist(err) {
		fmt.Fprintf(os.Stderr, "[Error] Missing %s\n", layoutPath)
		os.Exit(1)
	}

	compiler := &Compiler{
		srcDir:     srcDir,
		outDir:     outDir,
		layoutPath: layoutPath,
	}

	ok := compiler.BuildOnce()
	if !watch {
		if !ok {
			os.Exit(2)
		}
		return
	}

	fmt.Println("[Watch] Watching for changes…")

	watcher, err := fsnotify.NewWatcher()
	if err != nil {
		log.Fatal(err)
	}
	defer watcher.Close()

	done := make(chan bool)

	pending := make(map[string]bool)
	timer := time.NewTimer(0)
	<-timer.C // drain initial fire

	go func() {
		for {
			select {
			case event := <-watcher.Events:
				if strings.HasSuffix(event.Name, ".tmp") {
					continue
				}
				pending[event.Name] = true
				timer.Reset(150 * time.Millisecond)

			case <-timer.C:
				if len(pending) > 0 {
					pending = make(map[string]bool)
					compiler.BuildOnce()
					timer.Reset(150 * time.Millisecond)
				}

			case err := <-watcher.Errors:
				if err != nil {
					fmt.Printf("[Error] %v\n", err)
				}
			}
		}
	}()

	_ = filepath.Walk(srcDir, func(path string, info os.FileInfo, err error) error {
		if err == nil && info.IsDir() {
			watcher.Add(path)
		}
		return nil
	})

	<-done
}

func (c *Compiler) BuildOnce() bool {
	fmt.Printf("[Build] %s\n", time.Now().Format("15:04:05"))

	if err := os.MkdirAll(c.outDir, 0755); err != nil {
		fmt.Printf("[Error] %v\n", err)
		return false
	}

	layoutHTML, err := os.ReadFile(c.layoutPath)
	if err != nil {
		fmt.Printf("[Error] %v\n", err)
		return false
	}

	// Parse layout and extract slots
	layoutDoc, err := goquery.NewDocumentFromReader(bytes.NewReader(layoutHTML))
	if err != nil {
		fmt.Printf("[Error] parsing layout: %v\n", err)
		return false
	}

	var slots []SlotSpec
	layoutDoc.Find("[slot]").Each(func(i int, s *goquery.Selection) {
		name, _ := s.Attr("slot")
		mode, _ := s.Attr("slot-mode")
		if mode == "" {
			mode = "html"
		}
		tag := goquery.NodeName(s)
		inHead := s.Parents().Find("head").Length() > 0 || s.Closest("head").Length() > 0

		slots = append(slots, SlotSpec{
			Name:      name,
			Mode:      mode,
			LayoutTag: tag,
			InHead:    inHead,
		})
	})

	if len(slots) == 0 {
		fmt.Println("[Warn] No slots in _layout.html. Nothing to merge.")
	}

	overallOk := true

	// Process each HTML page
	entries, err := os.ReadDir(c.srcDir)
	if err != nil {
		fmt.Printf("[Error] %v\n", err)
		return false
	}

	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}

		fileName := entry.Name()
		if !strings.HasSuffix(fileName, ".html") || fileName == "_layout.html" {
			continue
		}

		pagePath := filepath.Join(c.srcDir, fileName)
		pageHTML, err := os.ReadFile(pagePath)
		if err != nil {
			fmt.Printf("[Error] %v\n", err)
			overallOk = false
			continue
		}

		pageDoc, err := goquery.NewDocumentFromReader(bytes.NewReader(pageHTML))
		if err != nil {
			fmt.Printf("[Error] parsing %s: %v\n", fileName, err)
			overallOk = false
			continue
		}

		// Map page slots
		pageMap := make(map[string]*goquery.Selection)
		pageDoc.Find("[for-slot]").Each(func(i int, s *goquery.Selection) {
			name, _ := s.Attr("for-slot")
			if _, exists := pageMap[name]; !exists {
				pageMap[name] = s
			}
		})

		// Check for unknown slots
		layoutNames := make(map[string]bool)
		for _, slot := range slots {
			layoutNames[slot.Name] = true
		}

		var extra []string
		for slotName := range pageMap {
			if !layoutNames[slotName] {
				extra = append(extra, slotName)
			}
		}

		if len(extra) > 0 {
			fmt.Printf("[Error] %s has unknown slots: %s\n", fileName, strings.Join(extra, ", "))
			overallOk = false
			// Still normalize but don't build
		}

		// Normalize: ensure all slots present and in correct order
		changed := false
		var normalizedHead, normalizedBody []*goquery.Selection

		for _, spec := range slots {
			provider, exists := pageMap[spec.Name]
			if !exists {
				// Auto-create empty provider
				provider = createEmptyProvider(pageDoc, spec)
				changed = true
				fmt.Printf("[AutoAdd] %s: inserted missing slot '%s'\n", fileName, spec.Name)
			}

			if spec.InHead {
				normalizedHead = append(normalizedHead, provider)
			} else {
				normalizedBody = append(normalizedBody, provider)
			}
		}

		// Check if order changed
		existingHead := pageDoc.Find("head [for-slot]")
		existingBody := pageDoc.Find("body [for-slot]")

		if existingHead.Length() != len(normalizedHead) || existingBody.Length() != len(normalizedBody) {
			changed = true
		}

		if changed {
			// Remove old slots
			pageDoc.Find("[for-slot]").Remove()

			// Add normalized slots
			for _, sel := range normalizedHead {
				html, _ := goquery.OuterHtml(sel)
				pageDoc.Find("head").AppendHtml(html)
			}
			for _, sel := range normalizedBody {
				html, _ := goquery.OuterHtml(sel)
				pageDoc.Find("body").AppendHtml(html)
			}

			newHTML, _ := pageDoc.Html()
			os.WriteFile(pagePath, []byte("<!DOCTYPE html><html>"+newHTML+"</html>"), 0644)
			fmt.Printf("[Normalized] Updated %s\n", fileName)
		}

		if len(extra) > 0 {
			continue
		}

		// Merge with layout
		outDoc, err := goquery.NewDocumentFromReader(bytes.NewReader(layoutHTML))
		if err != nil {
			fmt.Printf("[Error] merging %s: %v\n", fileName, err)
			overallOk = false
			continue
		}

		for _, spec := range slots {
			provider := pageDoc.Find(fmt.Sprintf("[for-slot='%s']", spec.Name)).First()
			if provider.Length() == 0 {
				continue
			}

			layoutEl := outDoc.Find(fmt.Sprintf("[slot='%s']", spec.Name)).First()

			switch spec.Mode {
			case "text":
				layoutEl.SetText(provider.Text())
			default:
				if strings.HasPrefix(spec.Mode, "attr:") {
					attrName := spec.Mode[5:]
					val, _ := provider.Attr(attrName)
					layoutEl.SetAttr(attrName, val)
				} else {
					html, _ := goquery.OuterHtml(provider)
					layoutEl.SetHtml(html)
				}
			}
		}

		outHTML, _ := outDoc.Html()
		destPath := filepath.Join(c.outDir, fileName)
		os.WriteFile(destPath, []byte("<!DOCTYPE html><html>"+outHTML+"</html>"), 0644)
		fmt.Printf("✔ Built %s\n", fileName)
	}

	c.copyAssetsDiff()
	fmt.Println("[Build] Complete.\n")

	return overallOk
}

func createEmptyProvider(doc *goquery.Document, spec SlotSpec) *goquery.Selection {
	tag := "section"
	switch spec.LayoutTag {
	case "title":
		tag = "title"
	case "meta":
		tag = "meta"
	}

	html := fmt.Sprintf(`<%s for-slot="%s"></%s>`, tag, spec.Name, tag)
	selection, err := goquery.NewDocumentFromReader(bytes.NewReader([]byte(html)))
	if err != nil {
		return nil
	}
	return selection.Find(tag).First()
}

func (c *Compiler) copyAssetsDiff() {
	filepath.Walk(c.srcDir, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return nil
		}

		fileName := filepath.Base(path)
		if strings.HasSuffix(fileName, ".html") {
			return nil
		}

		rel, _ := filepath.Rel(c.srcDir, path)
		dest := filepath.Join(c.outDir, rel)

		os.MkdirAll(filepath.Dir(dest), 0755)

		// Copy if doesn't exist or hash differs
		needsCopy := true
		if dInfo, err := os.Stat(dest); err == nil {
			if fileHashEqual(path, dest) {
				needsCopy = false
			} else if info.Size() == dInfo.Size() {
				needsCopy = true
			}
		}

		if needsCopy {
			src, _ := os.Open(path)
			defer src.Close()
			dst, _ := os.Create(dest)
			defer dst.Close()
			io.Copy(dst, src)
			fmt.Printf("📁 Copied %s\n", rel)
		}

		return nil
	})
}

func fileHashEqual(a, b string) bool {
	hashA, err1 := fileHash(a)
	hashB, err2 := fileHash(b)
	return err1 == nil && err2 == nil && bytes.Equal(hashA, hashB)
}

func fileHash(path string) ([]byte, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	h := sha256.New()
	_, err = io.Copy(h, f)
	if err != nil {
		return nil, err
	}

	return h.Sum(nil), nil
}
