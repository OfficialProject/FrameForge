<div align="center">

# ◈ FrameForge

**From raw video to a traceable research corpus.**

A local-first Rust pipeline for discovering videos, filtering candidates, preserving source evidence, sampling every second, optionally reading on-screen text, extracting concepts, and validating the resulting corpus.

[![Rust 2024](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](https://www.rust-lang.org/)
[![Version](https://img.shields.io/badge/version-1.0.0-2ea44f)](#status)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

</div>

---

## The idea

Video contains far more information than a title or transcript can capture.

FrameForge turns that information into **structured, inspectable evidence** without throwing away the source material that produced it.

The pipeline is designed for research tasks such as:

- tutorial and course analysis
- educational content discovery
- technical walkthroughs
- training-material research
- visual demonstration analysis
- creator or channel research
- single-video investigation
- domain-specific knowledge extraction

The engine is intentionally **profile-driven**. The default profile is general-purpose, while your own JSON profile can redefine what the classifier considers relevant.

> **Preserve the evidence first. Analyze it second.**

---

## What FrameForge does

| Stage | What happens |
| --- | --- |
| **Discover** | Finds videos from YouTube channels or accepts individual YouTube video URLs. |
| **Deduplicate** | Merges repeated video IDs while preserving source-channel provenance. |
| **Classify** | Uses weighted profile keywords for fast recall-first filtering. |
| **Retain** | Keeps educational, uncertain, and otherwise relevant candidates for research; clearly negative candidates are excluded. |
| **Metadata** | Captures title, description, duration, and upload date. |
| **Transcript** | Retrieves available English WebVTT captions when available. |
| **Visual** | Samples the entire retained video at **1 frame per second**. |
| **OCR** | Optionally runs Tesseract over every sampled frame. |
| **Evidence** | Links concepts back to titles, metadata, transcript timestamps, and OCR timestamps. |
| **Catalog** | Aggregates concepts across retained videos and calculates evidence-weighted importance. |
| **Learning order** | Produces a prerequisite-aware ordering of discovered concepts. |
| **Validation** | Checks schemas, provenance, coverage, paths, files, state, and cross-artifact consistency before success. |

### One important design choice

The expensive visual stage happens **after classification**.

That means a large channel can be screened cheaply before FrameForge downloads and analyzes the videos that survive the filter.

---

## Pipeline at a glance

```text
YouTube sources
      │
      ▼
Discovery ──► Deduplication
                    │
                    ▼
             Profile classifier
              ┌─────┴─────┐
              │           │
           reject       retain
                          │
             ┌────────────┼────────────┐
             ▼            ▼            ▼
          Metadata     Transcript    Video media
             │            │            │
             └────────────┴────────────┘
                          │
                          ▼
                   1 FPS visual layer
                          │
                    optional OCR
                          │
                          ▼
                 Evidence + concepts
                          │
                          ▼
                 Research catalog
                          │
                          ▼
                     Validation
                          │
                          ▼
                 Structured corpus
```

---

## Quick start

### 1. Install the runtime

You need:

- Rust 1.85 or newer
- [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- [Deno](https://deno.com/)
- [FFmpeg](https://ffmpeg.org/)
- FFprobe (included with FFmpeg)
- [Tesseract](https://github.com/tesseract-ocr/tesseract) only if OCR is enabled

For YouTube extraction, use a current yt-dlp build with its JavaScript challenge support available.

### 2. Add research sources

Put one YouTube channel URL or individual video URL on each line of `sources.txt`.

```text
https://www.youtube.com/@example
https://www.youtube.com/watch?v=VIDEO_ID
```

Blank lines and lines beginning with `#` are ignored.

### 3. Run FrameForge

```bash
cargo run --release -- run
```

That is the entire default workflow.

---

## CLI

### Discover candidates

```bash
cargo run --release -- discover
```

Discovers candidates without building the research corpus.

### Run research

```bash
cargo run --release -- run
```

Runs discovery, classification, evidence retrieval, visual sampling, optional OCR, concept extraction, catalog generation, and final validation.

### Validate an existing corpus

```bash
cargo run --release -- validate
```

Validates an existing output directory without reprocessing the videos.

### Force refresh

```bash
cargo run --release -- run --force
```

Ignores reusable classification and research caches and processes retained videos again.

---

## Configuration

FrameForge keeps configuration deliberately close to the surface.

### Command-line options

Every `run`, `discover`, and `validate` command accepts the relevant path overrides.

```bash
cargo run --release -- run \
  --sources sources.txt \
  --profile profiles/default.json \
  --root .frameforge \
  --output frameforge-output
```

### Environment variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `FRAMEFORGE_SOURCES` | `sources.txt` | Source list |
| `FRAMEFORGE_PROFILE` | `profiles/default.json` | Classification profile |
| `FRAMEFORGE_ROOT` | `.frameforge` | Persistent state and classification cache |
| `FRAMEFORGE_OUTPUT` | `frameforge-output` | Research corpus |
| `FRAMEFORGE_CLASSIFY_CONCURRENCY` | `6` | Concurrent classification workers |
| `FRAMEFORGE_RESEARCH_CONCURRENCY` | `2` | Concurrent research workers |
| `FRAMEFORGE_FORCE_REFRESH` | unset | Set to `1` to force a refresh |
| `FRAMEFORGE_OCR` | disabled | Set to `1` to enable OCR |
| `FRAMEFORGE_KEEP_VIDEO` | disabled | Set to `1` to retain downloaded source media |

Concurrency values are bounded internally so an accidental environment setting cannot create an uncontrolled worker pool.

---

## Profiles

Profiles are ordinary JSON files.

The shipped `default` profile is intentionally broad: it favors language associated with instruction, learning, analysis, technique, strategy, workflows, fundamentals, and practical improvement while penalizing obvious entertainment-only patterns.

A profile contains:

- a name and description
- positive weighted terms
- negative weighted terms
- thresholds for retaining or rejecting candidates
- a stronger threshold for short-form content

Example:

```json
{
  "name": "my-profile",
  "description": "A custom research filter.",
  "classification": {
    "educational_threshold": 6,
    "short_educational_threshold": 8,
    "non_educational_threshold": -3,
    "short_penalty": -1,
    "positive": [
      ["tutorial", 5, "tutorial"],
      ["workflow", 4, "workflow"]
    ],
    "negative": [
      ["montage", -7, "montage"]
    ]
  }
}
```

Then run it without changing the Rust source:

```bash
cargo run --release -- run --profile profiles/my-profile.json
```

### Classification behavior

FrameForge uses two passes:

1. **Title pass** — inexpensive scoring from the title.
2. **Evidence pass** — uncertain candidates can trigger metadata and transcript retrieval before the final classification.

Matching is normalized to word boundaries, so a keyword does not accidentally match an unrelated substring.

Classification caches include a fingerprint of the profile file. Change the profile, and stale classification results are not treated as current.

---

## Evidence and provenance

Every retained video receives a structured evidence package.

```text
.frameforge/
├── classifications/
│   └── <video-id>.json
└── state/
    └── <video-id>.json

frameforge-output/
├── manifest.json
├── research_catalog.json
└── <video-id>/
    ├── classification.json
    ├── video.json
    ├── metadata.json
    ├── transcript.json
    ├── visual.json
    ├── ocr.json
    ├── concepts.json
    ├── analysis.json
    └── frames/
        ├── frame-000000.jpg
        ├── frame-000001.jpg
        └── ...
```

The important relationship is:

```text
source
  ↓
metadata / transcript / frames / OCR
  ↓
evidence
  ↓
concepts
  ↓
cross-video catalog
```

The generated analysis also records the profile fingerprint, pipeline version, source URL, video ID, sampling mode, OCR state, and available runtime tool versions.

---

## Exhaustive visual sampling

For every retained video, FrameForge uses FFmpeg to build a **1 FPS frame layer across the full duration**.

There is no arbitrary frame-count cap.

That makes the visual layer useful for information that may never appear in captions, including:

- diagrams and slides
- software interfaces
- charts and tables
- demonstrations
- settings panels
- annotations
- visual examples
- other transient on-screen information

Frame records contain:

- sequential index
- timestamp
- relative frame path

The validator enforces the 1 FPS invariant and checks that every referenced frame exists inside the expected output tree.

---

## Optional OCR

OCR is **off by default** because it is one of the more expensive stages.

Enable it with:

```bash
FRAMEFORGE_OCR=1 cargo run --release -- run
```

When enabled:

- Tesseract must be installed and available on `PATH`
- every sampled frame is eligible for OCR
- OCR timestamps are tied to sampled frame timestamps
- OCR failures are surfaced rather than silently discarded
- OCR caches are invalidated when the OCR configuration changes

If OCR is enabled and Tesseract is unavailable, FrameForge stops early with an actionable error.

---

## Resumability and caching

Processing state is stored under `.frameforge/`.

Each video advances through:

```text
metadata
   ↓
downloaded
   ↓
visual
   ↓
ocr
   ↓
complete
```

A failed run can resume from valid earlier stages instead of automatically repeating expensive work.

Caches are not trusted blindly. Before reuse, FrameForge checks the information needed to establish that the cached artifact still belongs to the current:

- video
- source URL
- profile
- profile fingerprint
- pipeline version
- OCR configuration
- visual frame set

---

## Validation

Validation is part of the pipeline, not an optional afterthought.

Before a successful run is reported, FrameForge verifies:

- manifest and catalog schemas
- exact pipeline/schema versions
- profile consistency
- video identity and source provenance
- classification fingerprints
- processing state and counters
- required per-video artifacts
- transcript timestamps and text
- OCR timestamps and text
- exhaustive frame counts and timestamps
- safe relative frame paths
- frame-file existence and containment
- concept evidence structure
- catalog concept references
- prerequisite relationships
- prerequisite-aware learning order

The validator also rejects malformed numeric values, including non-finite timestamps and scores.

A green run therefore means the **generated corpus passed an integrity check**, not merely that the process reached the end of a loop.

---

## Output catalog

`research_catalog.json` aggregates concepts across retained videos.

Each concept can include:

- category
- difficulty level
- mention count
- evidence score
- cross-video coverage
- prerequisites
- importance score

The learning order is topological: prerequisites must appear before dependent concepts. If the prerequisite graph is cyclic or unsatisfiable, catalog generation fails instead of producing an invalid order.

---

## Reliability and security posture

FrameForge is designed to fail closed around important integrity boundaries.

Notable protections include:

- atomic JSON/state writes
- exclusive temporary-file creation
- profile fingerprints for cache invalidation
- strict video-ID validation
- path traversal checks
- canonicalized filesystem containment checks
- symlink escape detection
- strict frame-index and timestamp validation
- finite numeric validation
- explicit external-command failure handling
- reproducible `Cargo.lock`
- RustSec dependency auditing
- pinned GitHub Actions
- least-privilege workflow permissions
- Linux, macOS, and Windows test coverage

The repository's CI also checks that the public source surface remains domain-neutral.

---

## Project layout

```text
FrameForge/
├── .github/
│   ├── dependabot.yml
│   └── workflows/ci.yml
├── profiles/
│   └── default.json
├── src/
│   ├── atomic.rs
│   ├── catalog.rs
│   ├── classifier.rs
│   ├── main.rs
│   ├── model.rs
│   ├── progress.rs
│   ├── registry.rs
│   ├── transcript.rs
│   ├── validation.rs
│   ├── visual.rs
│   └── ytdlp.rs
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
└── sources.txt
```

---

## Requirements at a glance

| Component | Role |
| --- | --- |
| **Rust 1.85+** | Builds and runs FrameForge |
| **yt-dlp** | Discovery, metadata, captions, and media retrieval |
| **Deno 2.x** | JavaScript challenge support for the yt-dlp workflow |
| **FFmpeg / FFprobe** | Full-duration visual sampling and media inspection |
| **Tesseract** | Optional OCR |
| **YouTube access** | Current discovery source |

FrameForge does not require a database, cloud account, API key, or hosted backend for its core workflow.

---

## Status

### **1.0.0 — stable foundation**

FrameForge 1.0.0 provides a complete local pipeline for turning supported video sources into validated, provenance-preserving research data.

The architecture is intentionally modular so future releases can add richer extraction, additional source adapters, structured exports, or external analysis layers without discarding the evidence model.

---

<div align="center">

**FrameForge 1.0.0**

*Preserve the footage. Preserve the evidence. Find the signal.*

</div>
