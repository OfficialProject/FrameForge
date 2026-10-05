<div align="center">

# ◈ FrameForge

**Turn raw video into structured, traceable research evidence.**

A configurable Rust pipeline for **video discovery, classification, metadata, transcripts, exhaustive visual sampling, OCR, evidence extraction, concept analysis, and resumable research**.

[![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-research--ready-success)](#project-status)

</div>

---

## ✦ What is FrameForge?

FrameForge is built around a simple idea:

> **Video is data. FrameForge turns it into researchable data.**

Instead of treating a video as something to watch once and forget, FrameForge preserves the evidence needed to inspect, compare, and analyze it later.

It can be used for:

- educational-video research;
- tutorial and course analysis;
- content research;
- visual demonstrations;
- training material;
- technical walkthroughs;
- domain-specific knowledge extraction;
- individual-video investigation;
- whole-channel research.

The engine is **domain-agnostic by design**. The repository ships with a general-purpose `default` profile, and additional profiles can be configured without changing the engine.

---

## ⚙️ Pipeline

```
        Sources
           │
     ┌─────┴─────┐
     │           │
  Channels    Videos
     │           │
     └─────┬─────┘
           ▼
      Deduplication
           │
           ▼
   Profile-Based Filter
      │          │
   Reject       Keep
                  │
          ┌───────┴────────┐
          ▼                ▼
       Metadata        Transcript
          │                │
          └───────┬────────┘
                  ▼
       Exhaustive 1 FPS Frames
                  │
             Optional OCR
                  │
                  ▼
         Evidence + Concepts
                  │
                  ▼
       Cross-Video Aggregation
                  │
                  ▼
       Prerequisite-Aware Order
                  │
                  ▼
             Validation
                  │
                  ▼
          Research Corpus
```

**The expensive visual stage happens only after filtering.**

That keeps the pipeline practical without sacrificing the evidence model.

---

## ◇ Core principles

### Recall with precision

Filtering is deliberately customizable to a users specific needs. By default, strong candidates are retained, and uncertain candidates remain eligible for research rather than being silently discarded.

### Shorts are eligible

Short-form videos are not automatically excluded. Whether they are retained is controlled by the selected profile or configured filters.

### Stable identity

Videos are deduplicated by video ID, preserving source provenance when the same video is encountered through multiple inputs.

### Exhaustive visual evidence

Retained videos are sampled at **1 frame per second for their entire duration**.

There is no arbitrary frame cap.

### Provenance first

Every research result can be traced back to its source video, metadata, transcript, OCR, frames, classification, profile, and tool versions.

### Resumable by stage

Processing state survives failures. Completed metadata, visual extraction, and OCR work can be reused instead of repeated unnecessarily.

### Validate before success

FrameForge does not declare a corpus complete merely because processing stopped with exit code zero. Required artifacts, state, provenance, frame coverage, and catalog references are validated before success is reported.

---

## ◈ Profiles

Profiles make the filtering behavior configurable without changing the engine.

### `default`

The default profile is intended for general users.

It recognizes common educational/informational language such as:

- tutorials;
- guides;
- walkthroughs;
- analysis;
- lessons;
- techniques;
- workflows;
- strategy;
- fundamentals.

It also down-ranks obvious entertainment-only material such as montages, highlights, vlogs, giveaways, and stream/reaction content.

### Create your own

Profiles are plain JSON.

Copy `profiles/default.json`, change the keyword weights and thresholds, and run FrameForge with your profile.

```bash
cargo run --release -- run --profile profiles/my-profile.json
```

No Rust changes are required for normal filter customization.

---

## ▶ Inputs

Put one source per line in `sources.txt`.

FrameForge accepts:

**A channel**
```text
https://www.youtube.com/@example
```

**An individual video**
```text
https://www.youtube.com/watch?v=VIDEO_ID
```

This means you can research an entire creator, a curated collection of creators, or a single video without changing the pipeline.

---

## 🧪 Usage

### Discover

```bash
cargo run --release -- discover
```

### Run

```bash
cargo run --release -- run
```

### Validate an existing corpus

```bash
cargo run --release -- validate
```

### Force a fresh run

```bash
cargo run --release -- run --force
```

---

## ⚙️ Configuration

| Setting | Default | Purpose |
|---|---:|---|
| Profile | `profiles/default.json` | Classification/filter behavior |
| Sources | `sources.txt` | Channels and individual videos |
| State | `.frameforge/` | Persistent processing state |
| Output | `frameforge-output/` | Research corpus |
| Classification workers | 6 | Lightweight concurrent filtering |
| Research workers | 2 | Expensive video processing |
| OCR | enabled | OCR every retained frame |
| Source retention | disabled | Remove downloaded source media after extraction |
| Visual sampling | 1 FPS | Exhaustive full-duration sampling |
| Shorts | eligible | Profile decides whether they are retained |

Environment variables:

```text
FRAMEFORGE_SOURCES
FRAMEFORGE_PROFILE
FRAMEFORGE_ROOT
FRAMEFORGE_OUTPUT
FRAMEFORGE_CLASSIFY_CONCURRENCY
FRAMEFORGE_RESEARCH_CONCURRENCY
FRAMEFORGE_FORCE_REFRESH=1
FRAMEFORGE_OCR=0
FRAMEFORGE_KEEP_VIDEO=1
```

---

## ◈ Evidence model

Each retained video produces a structured evidence package:

```
.research/
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
        ├── 000000.jpg
        ├── 000001.jpg
        └── ...
```

The corpus preserves the relationship between:

**source → evidence → concepts → aggregate analysis**

rather than reducing the source to an opaque summary.

---

## 🔬 Why the visual stage is exhaustive

FrameForge samples retained videos at **1 FPS across the complete duration**.

That creates a predictable visual evidence layer for:

- demonstrations;
- diagrams;
- settings;
- charts;
- HUDs;
- slides;
- software interfaces;
- physical technique;
- visual examples;
- on-screen annotations.

OCR is optional, but when enabled it is eligible for every sampled frame.

---

## ♻️ Resumability

A video moves through persistent stages:

```
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

If a run fails after visual extraction, FrameForge can reuse the valid visual cache on the next run.

This is especially important for long videos where 1 FPS extraction may produce thousands of frames.

---

## 🛡️ Validation

Before reporting success, FrameForge verifies:

- manifest structure;
- research catalog structure;
- completed processing state;
- required per-video artifacts;
- schema versions;
- provenance/video identity;
- exhaustive visual coverage;
- frame-file existence;
- concept evidence structure;
- learning-order references.

A successful run therefore means **the generated corpus passed integrity checks**, not merely that the process reached the end of the loop.

---

## 🧱 Architecture

| Module | Responsibility |
|---|---|
| `classifier.rs` | Profile-driven recall-first classification |
| `catalog.rs` | Profile-aware concept extraction and aggregation |
| `model.rs` | Persistent schemas |
| `registry.rs` | Stage-aware state and manifest |
| `transcript.rs` | WebVTT parsing |
| `visual.rs` | 1 FPS extraction and OCR |
| `ytdlp.rs` | Source discovery, metadata, transcripts, downloads |
| `validation.rs` | Corpus integrity validation |
| `progress.rs` | Terminal progress |
| `main.rs` | CLI and orchestration |

---

## 🛠 Requirements

- **Rust 2024**
- **yt-dlp** with YouTube EJS support
- **Deno 2.6+**
- **FFmpeg**
- **FFprobe**
- **Tesseract** for OCR

---

## 🗺 Roadmap

FrameForge is intentionally structured so future work can add:

- richer semantic classification;
- timestamped evidence graphs;
- speaker/source attribution;
- cross-source agreement;
- contradiction detection;
- Parquet/DuckDB exports;
- research dashboards;
- additional video sources.

The central design rule remains:

> **Future intelligence should build on preserved evidence, not replace it.**

---

## ✦ Project status

**FrameForge core pipeline — research-ready.**

The repository provides a generic video research engine with configurable profiles and no domain-specific assumptions in the default workflow.

---

<div align="center">

**FrameForge**  
*Parse the footage. Preserve the evidence. Find the signal.*

</div>
